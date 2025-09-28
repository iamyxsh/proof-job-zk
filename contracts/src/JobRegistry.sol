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
        bytes calldata result,
        bytes calldata seal,
        bytes32 journalDigest
    ) external {
        Job storage job = jobs[jobId];

        if (!job.exists) revert JobNotFound();
        if (completed[jobId]) revert JobAlreadyCompleted();

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
