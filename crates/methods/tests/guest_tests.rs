use methods::{GUEST_ELF, GUEST_ID};
use risc0_zkvm::{default_executor, ExecutorEnv};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct JobInput {
    job_id: [u8; 32],
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct JobOutput {
    job_id: [u8; 32],
    result: Vec<u8>,
    result_hash: [u8; 32],
}

#[test]
fn guest_id_is_populated() {
    assert_ne!(GUEST_ID, [0u32; 8]);
}

#[test]
fn guest_computes_fib_10() {
    let input = JobInput {
        job_id: [0xaa; 32],
        data: 10u64.to_le_bytes().to_vec(),
    };

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();

    let executor = default_executor();
    let session = executor.execute(env, GUEST_ELF).unwrap();
    let output: JobOutput = session.journal.decode().unwrap();

    assert_eq!(output.job_id, [0xaa; 32]);
    let result = u64::from_le_bytes(output.result[..8].try_into().unwrap());
    assert_eq!(result, 55);
}

#[test]
fn guest_computes_fib_zero() {
    let input = JobInput {
        job_id: [0xbb; 32],
        data: 0u64.to_le_bytes().to_vec(),
    };

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();

    let executor = default_executor();
    let session = executor.execute(env, GUEST_ELF).unwrap();
    let output: JobOutput = session.journal.decode().unwrap();

    let result = u64::from_le_bytes(output.result[..8].try_into().unwrap());
    assert_eq!(result, 0);
}

#[test]
fn guest_computes_fib_one() {
    let input = JobInput {
        job_id: [0xcc; 32],
        data: 1u64.to_le_bytes().to_vec(),
    };

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();

    let executor = default_executor();
    let session = executor.execute(env, GUEST_ELF).unwrap();
    let output: JobOutput = session.journal.decode().unwrap();

    let result = u64::from_le_bytes(output.result[..8].try_into().unwrap());
    assert_eq!(result, 1);
}

#[test]
fn guest_preserves_job_id() {
    let job_id = [0x42; 32];
    let input = JobInput {
        job_id,
        data: 5u64.to_le_bytes().to_vec(),
    };

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();

    let executor = default_executor();
    let session = executor.execute(env, GUEST_ELF).unwrap();
    let output: JobOutput = session.journal.decode().unwrap();

    assert_eq!(output.job_id, job_id);
    let result = u64::from_le_bytes(output.result[..8].try_into().unwrap());
    assert_eq!(result, 5);
}
