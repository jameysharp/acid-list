use acid_list::{AcidList, LinkIndex};
use std::io;

fn main() -> io::Result<()> {
    let mut args = std::env::args();
    args.next(); // skip program name

    let path = args.next().ok_or(io::ErrorKind::InvalidInput)?;
    let file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;

    let list = AcidList::<[u8; 32]>::open(file)?;
    let heads = list.header().heads;

    for head in 0..heads {
        println!("List #{}:", head + 1);
        let head = LinkIndex::Head(head);

        let mut prev_idx = head;
        let mut prev_neighbors = list.neighbors(prev_idx);
        while let LinkIndex::Node(cur) = prev_neighbors.next {
            let cur_idx = LinkIndex::Node(cur);
            let cur_neighbors = list.neighbors(cur_idx);

            println!("  node #{}: {:?}", cur, list.get(cur));
            assert!(cur_neighbors.previous == prev_idx);

            prev_idx = cur_idx;
            prev_neighbors = cur_neighbors;
        }
        assert!(prev_neighbors.next == head);
        assert!(list.neighbors(head).previous == prev_idx);

        println!();
    }

    list.close()
}
