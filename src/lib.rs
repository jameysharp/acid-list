#![warn(nonstandard_style)]

mod syscall;

use std::fmt;
use std::fs;
use std::io;
use std::io::{Seek, Write};
use std::marker::PhantomData;
use std::mem;
use std::os::unix::io::AsRawFd;

#[derive(Debug)]
enum Error {
    NotInitialized,
    WrongArchitecture,
    WrongDataType,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NotInitialized => write!(f, "can't open uninitialized backing file"),
            Error::WrongArchitecture => write!(f, "can't open file from a different architecture"),
            Error::WrongDataType => write!(f, "can't open file with differently sized data"),
        }
    }
}

impl std::error::Error for Error {}

pub type NodeIndex = u32;
const HEAD_FLAG: NodeIndex = 1 << 31;

#[derive(Clone, PartialEq, Eq)]
pub enum LinkIndex {
    Node(NodeIndex),
    Head(NodeIndex),
}

impl Copy for LinkIndex {}

impl LinkIndex {
    fn to_node(self) -> NodeIndex {
        match self {
            LinkIndex::Node(n) => n,
            LinkIndex::Head(n) => n | HEAD_FLAG,
        }
    }

    fn from_node(idx: NodeIndex) -> Self {
        if idx & HEAD_FLAG == 0 {
            LinkIndex::Node(idx)
        } else {
            LinkIndex::Head(idx & !HEAD_FLAG)
        }
    }
}

// XXX: Header could be a caller-provided repr(C) type implementing a
// trait for the few things that AcidList actually needs from it,
// including one method for validating the header on open and returning
// a caller-meaningful error if that fails. Then applications could
// store extra metadata for interpreting list contents, or schema
// information to validate against the running version's schema.
#[repr(C)]
pub struct Header<T> {
    magic: u32,
    data_size: u32,
    pub heads: NodeIndex,
    pub nodes: NodeIndex,
    marker: PhantomData<Node<T>>,
}

const HEADER_MAGIC: u32 = 0x41434944; // "ACID"

fn align_to<T>(offset: u64) -> u64 {
    let align = mem::align_of::<T>() as u64;
    (offset + (align - 1)) / align * align
}

impl<T> Header<T> {
    pub fn new(heads: NodeIndex, nodes: NodeIndex) -> Self {
        Header {
            magic: HEADER_MAGIC,
            data_size: mem::size_of::<T>() as u32,
            heads,
            nodes,
            marker: PhantomData,
        }
    }

    fn heads_offset(&self) -> u64 {
        align_to::<Link>(mem::size_of_val(self) as u64)
    }

    fn nodes_offset(&self) -> u64 {
        align_to::<Node<T>>(self.heads_offset() + self.heads as u64 * mem::size_of::<Link>() as u64)
    }

    fn file_size(&self) -> u64 {
        self.nodes_offset() + self.nodes as u64 * mem::size_of::<Node<T>>() as u64
    }
}

#[repr(C)]
#[derive(Clone)]
struct Link {
    previous: NodeIndex,
    next: NodeIndex,
}

impl Copy for Link {}

#[derive(Clone)]
pub struct NodeNeighbors {
    pub previous: LinkIndex,
    pub next: LinkIndex,
}

impl Copy for NodeNeighbors {}

#[repr(C)]
struct Node<T> {
    link: Link,
    contents: T,
}

pub struct AcidList<T> {
    base: *mut libc::c_void,
    len: libc::size_t,
    marker: PhantomData<Node<T>>,
}

impl<T> AcidList<T> {
    pub fn create<P>(path: P, header: Header<T>) -> io::Result<Self>
    where
        P: AsRef<std::path::Path>,
    {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)?;
        file.set_len(header.file_size())?;
        file.write_all(unsafe {
            std::slice::from_raw_parts(
                &header as *const Header<T> as *const u8,
                mem::size_of_val(&header),
            )
        })?;

        // at this point we've established the invariants that open() checks for
        let mut list = AcidList::open(file)?;

        // initialize each list head to point to itself, making it empty
        for head_idx in 0..header.heads {
            let head_idx = LinkIndex::Head(head_idx).to_node();
            *list.link_mut(head_idx) = Link {
                previous: head_idx,
                next: head_idx,
            };
        }

        // put all the initially-allocated nodes in list 0
        if header.nodes > 0 {
            for node_idx in 0..header.nodes {
                *list.link_mut(node_idx) = Link {
                    previous: node_idx.wrapping_sub(1),
                    next: node_idx.wrapping_add(1),
                };
            }

            let head_idx = LinkIndex::Head(0).to_node();
            list.link_mut(0).previous = head_idx;
            list.link_mut(header.nodes - 1).next = head_idx;
            *list.link_mut(head_idx) = Link {
                previous: header.nodes - 1,
                next: 0,
            };
        }

        Ok(list)
    }

    pub fn open(mut file: fs::File) -> io::Result<Self> {
        let fd = file.as_raw_fd();
        syscall::flock(fd, libc::LOCK_EX)?;
        let len = file.seek(io::SeekFrom::End(0))?;
        if len > libc::size_t::max_value() as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                Error::WrongArchitecture,
            ));
        }

        let len = len as libc::size_t;

        // ensure that list.header() can be called without SIGBUS
        let expected_size = mem::size_of::<Header<T>>();
        if len < expected_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                Error::NotInitialized,
            ));
        }

        let list = AcidList {
            base: unsafe {
                syscall::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )?
            },
            len: len,
            marker: PhantomData,
        };

        let header = list.header();
        if header.magic != HEADER_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                Error::WrongArchitecture,
            ));
        }

        if header.data_size as usize != mem::size_of::<T>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                Error::WrongDataType,
            ));
        }

        let expected_size = header.file_size();
        if header.heads < 1
            || expected_size > usize::max_value() as u64
            || len != expected_size as usize
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                Error::NotInitialized,
            ));
        }

        Ok(list)
    }

    pub fn checkpoint(&self) -> io::Result<()> {
        unsafe { syscall::msync(self.base, self.len, libc::MS_SYNC) }
    }

    pub fn close(self) -> io::Result<()> {
        unsafe {
            syscall::munmap(self.base, self.len)?;
        }

        // don't let the last-chance Drop implementation run if the
        // caller explicitly calls close()
        mem::forget(self);
        Ok(())
    }

    pub fn header(&self) -> &Header<T> {
        let header = self.base as *const Header<T>;
        unsafe { &*header }
    }

    pub fn set(&mut self, idx: NodeIndex, value: T) {
        self.node_mut(idx).contents = value;
    }

    pub fn get(&self, idx: NodeIndex) -> &T {
        &self.node(idx).contents
    }

    pub fn neighbors(&self, idx: LinkIndex) -> NodeNeighbors {
        let link = self.link(idx);
        NodeNeighbors {
            previous: LinkIndex::from_node(link.previous),
            next: LinkIndex::from_node(link.next),
        }
    }

    pub fn move_before(&mut self, from_idx: NodeIndex, to_next_idx: LinkIndex) {
        assert!(LinkIndex::Node(from_idx) != to_next_idx);

        let from = self.link(LinkIndex::Node(from_idx));

        if from.next == to_next_idx.to_node() {
            // node is already in the requested spot
            return;
        }

        self.move_to(from_idx, from, Link {
            previous: self.link(to_next_idx).previous,
            next: to_next_idx.to_node(),
        });
    }

    pub fn move_after(&mut self, from_idx: NodeIndex, to_previous_idx: LinkIndex) {
        assert!(LinkIndex::Node(from_idx) != to_previous_idx);

        let from = self.link(LinkIndex::Node(from_idx));

        if from.previous == to_previous_idx.to_node() {
            // node is already in the requested spot
            return;
        }

        self.move_to(from_idx, from, Link {
            previous: to_previous_idx.to_node(),
            next: self.link(to_previous_idx).next,
        });
    }

    fn move_to(&mut self, from_idx: NodeIndex, from: Link, to: Link) {
        self.link_mut(from.next).previous = from.previous;
        self.link_mut(from.previous).next = from.next;
        *self.link_mut(from_idx) = to;
        self.link_mut(to.next).previous = from_idx;
        self.link_mut(to.previous).next = from_idx;
    }

    unsafe fn head_ptr(&self, idx: NodeIndex) -> *mut Link {
        assert!(idx < self.header().heads);
        let base = self.base as *mut u8;
        let heads = base.offset(self.header().heads_offset() as isize) as *mut Link;
        heads.offset(idx as isize)
    }

    fn head(&self, idx: NodeIndex) -> &Link {
        unsafe { &*self.head_ptr(idx) }
    }

    fn head_mut(&mut self, idx: NodeIndex) -> &mut Link {
        unsafe { &mut *self.head_ptr(idx) }
    }

    unsafe fn node_ptr(&self, idx: NodeIndex) -> *mut Node<T> {
        assert!(idx < self.header().nodes);
        let base = self.base as *mut u8;
        let nodes = base.offset(self.header().nodes_offset() as isize) as *mut Node<T>;
        nodes.offset(idx as isize)
    }

    fn node(&self, idx: NodeIndex) -> &Node<T> {
        unsafe { &*self.node_ptr(idx) }
    }

    fn node_mut(&mut self, idx: NodeIndex) -> &mut Node<T> {
        unsafe { &mut *self.node_ptr(idx) }
    }

    fn link(&self, idx: LinkIndex) -> Link {
        match idx {
            LinkIndex::Node(idx) => self.node(idx).link,
            LinkIndex::Head(idx) => *self.head(idx),
        }
    }

    fn link_mut(&mut self, idx: NodeIndex) -> &mut Link {
        match LinkIndex::from_node(idx) {
            LinkIndex::Node(idx) => &mut self.node_mut(idx).link,
            LinkIndex::Head(idx) => self.head_mut(idx),
        }
    }
}

impl<T> Drop for AcidList<T> {
    fn drop(&mut self) {
        unsafe {
            syscall::munmap(self.base, self.len).expect("AcidList::close");
        }
    }
}
