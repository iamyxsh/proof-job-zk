// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./interfaces/IJobRegistry.sol";
import "./interfaces/IRiscZeroVerifier.sol";

contract JobRegistry is IJobRegistry {
    struct Job {
        address owner;
        bytes payload;
        uint256 reward;
        uint256 deadline;
        bool exists;
    }

    mapping(bytes32 => Job) public jobs;
    mapping(bytes32 => bool) public completed;

    IRiscZeroVerifier public immutable verifier;
    bytes32 public immutable imageId;

    constructor(address _verifier, bytes32 _imageId) {
        verifier = IRiscZeroVerifier(_verifier);
        imageId = _imageId;
    }

    function submitJob(
        bytes32 jobId,
        bytes calldata payload,
        uint256 deadline
    ) external payable {
        if (jobs[jobId].exists) revert JobAlreadyExists();
        if (msg.value == 0) revert InsufficientReward();
        if (deadline <= block.timestamp) revert DeadlinePassed();

        jobs[jobId] = Job({
            owner: msg.sender,
            payload: payload,
            reward: msg.value,
            deadline: deadline,
            exists: true
        });

        emit JobSubmitted(jobId, msg.sender, msg.value, deadline);
    }

    function submitProof(
        bytes32 jobId,
        bytes calldata journal,
        bytes calldata seal
    ) external {
        Job storage job = jobs[jobId];

        if (!job.exists) revert JobNotFound();
        if (completed[jobId]) revert JobAlreadyCompleted();
        if (block.timestamp > job.deadline) revert DeadlinePassed();

        // Journal layout (raw bytes committed by guest):
        //   [0..32)   job_id
        //   [32..64)  payload_hash  (sha256 of input data)
        //   [64..)    result
        if (journal.length < 64) revert InvalidJournal();

        bytes32 journalJobId = bytes32(journal[0:32]);
        bytes32 journalPayloadHash = bytes32(journal[32:64]);
        bytes memory result = journal[64:];

        if (journalJobId != jobId) revert JobIdMismatch();
        if (journalPayloadHash != sha256(job.payload)) revert PayloadMismatch();

        bytes32 journalDigest = sha256(journal);
        verifier.verify(seal, imageId, journalDigest);

        completed[jobId] = true;

        uint256 reward = job.reward;

        (bool success, ) = msg.sender.call{value: reward}("");
        if (!success) revert TransferFailed();

        emit JobCompleted(jobId, msg.sender, result);
    }

    function expireJob(bytes32 jobId) external {
        Job storage job = jobs[jobId];

        if (!job.exists) revert JobNotFound();
        if (completed[jobId]) revert JobAlreadyCompleted();
        if (block.timestamp <= job.deadline) revert JobNotExpired();

        completed[jobId] = true;

        uint256 reward = job.reward;
        address owner = job.owner;

        (bool success, ) = owner.call{value: reward}("");
        if (!success) revert TransferFailed();

        emit JobExpired(jobId);
    }

    function getJob(bytes32 jobId) external view returns (
        address owner,
        bytes memory payload,
        uint256 reward,
        uint256 deadline,
        bool exists,
        bool isCompleted
    ) {
        Job storage job = jobs[jobId];
        return (
            job.owner,
            job.payload,
            job.reward,
            job.deadline,
            job.exists,
            completed[jobId]
        );
    }
}
