extern crate sled;
extern crate libc;

use std::thread;
use std::time::Duration;

const CYCLE: usize = 16; // 65536;

/// Verifies that the keys in the tree are correctly recovered.
/// Panics if they are incorrect.
/// Returns the key that should be resumed at, and the current cycle value.
fn verify(tree: &sled::Tree) -> (u32, u32) {
    // key 0 should always be the highest value, as that's where we increment
    // at some point, it might go down by one
    // it should never return, or go down again after that
    let mut iter = tree.iter();
    let highest = match iter.next() {
        None => return (0, 0),
        Some((_k, v)) => slice_to_u32(&*v),
    };

    let highest_vec = u32_to_vec(highest);

    println!("first value in tree: {}", highest);

    // find how far we got
    let mut contiguous: u32 = 0;
    let mut lowest = 0;
    for (mut k, v) in iter {
        if v == highest_vec {
            contiguous += 1;
        } else {
            k.reverse();
            println!(
                "different k: {} v: {}",
                slice_to_u32(&*k),
                slice_to_u32(&*v)
            );
            let expected = (highest - 1) % CYCLE as u32;
            let actual = slice_to_u32(&*v);
            assert_eq!(expected, actual);
            lowest = actual;
            break;
        }
    }

    println!("0 through {} are {}", contiguous, highest);

    let lowest_vec = u32_to_vec(lowest);

    // ensure nothing changes after this point
    let low_beginning = u32_to_vec(contiguous + 1);

    for (mut k, v) in tree.scan(&*low_beginning) {
        if v != lowest_vec {
            k.reverse();
            println!("k: {} v: {}", slice_to_u32(&*k), slice_to_u32(&*v));
        }
        assert_eq!(
            v,
            lowest_vec,
            "expected key {} to have value {}, instead it had value {}",
            slice_to_u32(&*k),
            lowest,
            slice_to_u32(&*v)
        );
    }

    (contiguous, highest)
}

fn u32_to_vec(u: u32) -> Vec<u8> {
    let buf: [u8; 4] = unsafe { std::mem::transmute(u) };
    buf.to_vec()
}

fn slice_to_u32(b: &[u8]) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(b);

    unsafe { std::mem::transmute(buf) }
}

fn main() {
    let config = sled::Config::default()
        .io_bufs(2)
        .blink_fanout(15)
        .page_consolidation_threshold(10)
        .cache_fixup_threshold(1)
        .cache_bits(6)
        .cache_capacity(128 * 1024 * 1024)
        .flush_every_ms(None)
        // drop io_buf_size to 1<<16, then 1<<17 to tease out
        // low hanging fruit more quickly
        .io_buf_size(100_000) // 1<<16 is 65k but might cause stalling
        .path("cycles".to_string())
        .snapshot_after_ops(1 << 10)
        .build();

    println!("restoring");
    let tree = config.tree();

    println!("verifying");
    let (key, highest) = verify(&tree);

    thread::spawn(|| {
        thread::sleep(Duration::from_millis(100));
        println!("raising SIGKILL");
        unsafe {
            libc::raise(9);
        }
    });

    println!("writing");

    let mut hu = ((highest as usize) * CYCLE) + key as usize;
    assert_eq!(hu % CYCLE, key as usize);
    assert_eq!(hu / CYCLE, highest as usize);

    loop {
        hu += 1;

        if hu / CYCLE >= CYCLE {
            hu = 0;
        }

        let mut key = u32_to_vec((hu % CYCLE) as u32);
        key.reverse();
        let value = u32_to_vec((hu / CYCLE) as u32);
        tree.set(key, value);
    }
}