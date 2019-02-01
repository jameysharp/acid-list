use acid_list::{AcidList, Header};
use std::io;

fn main() -> io::Result<()> {
    let mut args = std::env::args();
    args.next(); // skip program name

    let path = args.next().ok_or(io::ErrorKind::InvalidInput)?;
    let nodes = args
        .next()
        .ok_or(io::ErrorKind::InvalidInput)?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let heads = args.next().map_or(Ok(2), |s| {
        s.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    })?;

    let header = Header::<[u8; 32]>::new(heads, nodes);
    AcidList::create(path, header)?.close()
}
