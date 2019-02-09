use acid_list::{AcidList, LinkIndex};
use std::io;

fn main() -> io::Result<()> {
    let mut args = std::env::args();
    args.next(); // skip program name

    let path = args.next().ok_or(io::ErrorKind::InvalidInput)?;

    let from_idx = args
        .next()
        .ok_or(io::ErrorKind::InvalidInput)?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let direction = match &*args.next().ok_or(io::ErrorKind::InvalidInput)? {
        "before" => AcidList::move_before,
        "after" => AcidList::move_after,
        other => return Err(io::Error::new(io::ErrorKind::InvalidInput, other)),
    };

    let kind = match &*args.next().ok_or(io::ErrorKind::InvalidInput)? {
        "head" => LinkIndex::Head,
        "node" => LinkIndex::Node,
        other => return Err(io::Error::new(io::ErrorKind::InvalidInput, other)),
    };

    let to_idx = args
        .next()
        .ok_or(io::ErrorKind::InvalidInput)?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    let mut list = AcidList::<[u8; 32]>::open(file)?;

    direction(&mut list, from_idx, kind(to_idx));

    list.close()
}
