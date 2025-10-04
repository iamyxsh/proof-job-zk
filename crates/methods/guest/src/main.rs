use risc0_zkvm::guest::env;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
pub struct JobInput {
    pub job_id: [u8; 32],
    pub data: Vec<u8>,
}

fn main() {
    let input: JobInput = env::read();

    let n = if input.data.len() >= 8 {
        u64::from_le_bytes(input.data[..8].try_into().unwrap())
    } else {
        let mut buf = [0u8; 8];
        buf[..input.data.len()].copy_from_slice(&input.data);
        u64::from_le_bytes(buf)
    };

    let fib_result = fibonacci(n);
    let result = fib_result.to_le_bytes().to_vec();

    let payload_hash: [u8; 32] = Sha256::digest(&input.data).into();

    // Journal layout (raw bytes):
    //   [0..32)   job_id
    //   [32..64)  payload_hash  (sha256 of input data)
    //   [64..)    result        (variable length)
    env::commit_slice(&input.job_id);
    env::commit_slice(&payload_hash);
    env::commit_slice(&result);
}

fn fibonacci(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }

    let mut a: u64 = 0;
    let mut b: u64 = 1;

    for _ in 2..=n {
        let next = a.wrapping_add(b);
        a = b;
        b = next;
    }

    b
}
