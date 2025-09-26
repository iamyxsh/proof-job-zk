use risc0_zkvm::guest::env;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct JobInput {
    pub job_id: [u8; 32],
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct JobOutput {
    pub job_id: [u8; 32],
    pub result: Vec<u8>,
    pub result_hash: [u8; 32],
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

    let mut result_hash = [0u8; 32];
    result_hash[..result.len().min(32)].copy_from_slice(&result[..result.len().min(32)]);

    let output = JobOutput {
        job_id: input.job_id,
        result,
        result_hash,
    };
    env::commit(&output);
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
