// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IJobRegistry {
    event JobSubmitted(
        bytes32 indexed jobId,
        address indexed owner,
        uint256 reward,
        uint256 deadline
    );

    event JobCompleted(
        bytes32 indexed jobId,
        address indexed worker,
        bytes result
    );

    event JobExpired(bytes32 indexed jobId);

    error JobAlreadyExists();
    error JobNotFound();
    error JobAlreadyCompleted();
    error JobNotExpired();
    error DeadlinePassed();
    error InsufficientReward();
    error TransferFailed();
    error InvalidJournal();
    error JobIdMismatch();
    error PayloadMismatch();

    function submitJob(
        bytes32 jobId,
        bytes calldata payload,
        uint256 deadline
    ) external payable;

    function submitProof(
        bytes32 jobId,
        bytes calldata journal,
        bytes calldata seal
    ) external;

    function expireJob(bytes32 jobId) external;

    function getJob(bytes32 jobId) external view returns (
        address owner,
        bytes memory payload,
        uint256 reward,
        uint256 deadline,
        bool exists,
        bool isCompleted
    );
}
