use acid_list::{AcidList, LinkIndex};
use std::io;

fn main() -> io::Result<()> {
    let mut args = std::env::args();
    args.next(); // skip program name

    let path = args.next().ok_or(io::ErrorKind::InvalidInput)?;

    let mut access = Vec::with_capacity(args.len());

    for arg in args {
        access.push(arg.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?);
    }

    let file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    let mut list = AcidList::<[u8; 32]>::open(file)?;

    let mut prev = LinkIndex::Head(0);

    for idx in access.into_iter() {
        list.move_before(idx, prev);
        prev = LinkIndex::Node(idx);
    }

    list.close()
}
