// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Test.sol";
import "../src/JobRegistry.sol";
import "../src/interfaces/IJobRegistry.sol";
import "../src/mocks/MockRiscZeroVerifier.sol";

contract JobRegistryTest is Test {
    JobRegistry public registry;
    MockRiscZeroVerifier public mockVerifier;

    address public owner = address(0x1);
    address public worker = address(0x2);

    bytes32 public constant IMAGE_ID = bytes32(uint256(0x12345));
    bytes32 public constant JOB_ID = bytes32(uint256(0xabcdef));

    function setUp() public {
        mockVerifier = new MockRiscZeroVerifier();
        registry = new JobRegistry(address(mockVerifier), IMAGE_ID);

        vm.deal(owner, 10 ether);
        vm.deal(worker, 1 ether);
    }

    // Helper: build a valid journal for a given jobId and payload
    function _buildJournal(
        bytes32 jobId,
        bytes memory payload,
        bytes memory result
    ) internal pure returns (bytes memory) {
        bytes32 payloadHash = sha256(payload);
        return abi.encodePacked(jobId, payloadHash, result);
    }

    function test_submitJob_success() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;
        uint256 reward = 1 ether;

        vm.prank(owner);
        registry.submitJob{value: reward}(JOB_ID, payload, deadline);

        (
            address jobOwner,
            bytes memory jobPayload,
            uint256 jobReward,
            uint256 jobDeadline,
            bool exists,
            bool isCompleted
        ) = registry.getJob(JOB_ID);

        assertEq(jobOwner, owner);
        assertEq(jobPayload, payload);
        assertEq(jobReward, reward);
        assertEq(jobDeadline, deadline);
        assertTrue(exists);
        assertFalse(isCompleted);
    }

    function test_submitJob_emitsEvent() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;
        uint256 reward = 1 ether;

        vm.expectEmit(true, true, false, true);
        emit IJobRegistry.JobSubmitted(JOB_ID, owner, reward, deadline);

        vm.prank(owner);
        registry.submitJob{value: reward}(JOB_ID, payload, deadline);
    }

    function test_submitJob_revertIfAlreadyExists() public {
        bytes memory payload = hex"0a";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        vm.expectRevert(IJobRegistry.JobAlreadyExists.selector);
        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);
    }

    function test_submitJob_revertIfNoReward() public {
        bytes memory payload = hex"0a";
        uint256 deadline = block.timestamp + 1 hours;

        vm.expectRevert(IJobRegistry.InsufficientReward.selector);
        vm.prank(owner);
        registry.submitJob{value: 0}(JOB_ID, payload, deadline);
    }

    function test_submitJob_revertIfDeadlinePassed() public {
        bytes memory payload = hex"0a";
        uint256 deadline = block.timestamp - 1;

        vm.expectRevert(IJobRegistry.DeadlinePassed.selector);
        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);
    }

    function test_submitProof_success() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;
        uint256 reward = 1 ether;

        vm.prank(owner);
        registry.submitJob{value: reward}(JOB_ID, payload, deadline);

        bytes memory result = hex"3700000000000000";
        bytes memory journal = _buildJournal(JOB_ID, payload, result);
        bytes memory seal = hex"deadbeef";

        uint256 workerBalanceBefore = worker.balance;

        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, seal);

        assertEq(worker.balance, workerBalanceBefore + reward);

        (, , , , , bool isCompleted) = registry.getJob(JOB_ID);
        assertTrue(isCompleted);
    }

    function test_submitProof_emitsEvent() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        bytes memory result = hex"3700000000000000";
        bytes memory journal = _buildJournal(JOB_ID, payload, result);
        bytes memory seal = hex"deadbeef";

        vm.expectEmit(true, true, false, true);
        emit IJobRegistry.JobCompleted(JOB_ID, worker, result);

        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, seal);
    }

    function test_submitProof_revertIfJobNotFound() public {
        bytes memory journal = _buildJournal(JOB_ID, hex"0a", hex"37");
        vm.expectRevert(IJobRegistry.JobNotFound.selector);
        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, hex"deadbeef");
    }

    function test_submitProof_revertIfAlreadyCompleted() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        bytes memory journal = _buildJournal(JOB_ID, payload, hex"37");

        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, hex"deadbeef");

        vm.expectRevert(IJobRegistry.JobAlreadyCompleted.selector);
        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, hex"deadbeef");
    }

    function test_submitProof_revertIfJournalTooShort() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        // Journal shorter than 64 bytes
        bytes memory shortJournal = hex"aabbccdd";

        vm.expectRevert(IJobRegistry.InvalidJournal.selector);
        vm.prank(worker);
        registry.submitProof(JOB_ID, shortJournal, hex"deadbeef");
    }

    function test_submitProof_revertIfJobIdMismatch() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        // Build journal with a different job ID
        bytes32 wrongJobId = bytes32(uint256(0xdeadbeef));
        bytes memory journal = _buildJournal(wrongJobId, payload, hex"37");

        vm.expectRevert(IJobRegistry.JobIdMismatch.selector);
        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, hex"deadbeef");
    }

    function test_submitProof_revertIfPayloadMismatch() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        // Build journal with a different payload hash (wrong payload)
        bytes memory wrongPayload = hex"ff00000000000000";
        bytes memory journal = _buildJournal(JOB_ID, wrongPayload, hex"37");

        vm.expectRevert(IJobRegistry.PayloadMismatch.selector);
        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, hex"deadbeef");
    }

    function test_expireJob_success() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;
        uint256 reward = 1 ether;

        vm.prank(owner);
        registry.submitJob{value: reward}(JOB_ID, payload, deadline);

        uint256 ownerBalanceBefore = owner.balance;

        vm.warp(deadline + 1);

        registry.expireJob(JOB_ID);

        assertEq(owner.balance, ownerBalanceBefore + reward);

        (, , , , , bool isCompleted) = registry.getJob(JOB_ID);
        assertTrue(isCompleted);
    }

    function test_expireJob_emitsEvent() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        vm.warp(deadline + 1);

        vm.expectEmit(true, false, false, false);
        emit IJobRegistry.JobExpired(JOB_ID);

        registry.expireJob(JOB_ID);
    }

    function test_expireJob_revertIfNotExpired() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        vm.expectRevert(IJobRegistry.JobNotExpired.selector);
        registry.expireJob(JOB_ID);
    }

    function test_submitProof_revertIfDeadlinePassed() public {
        bytes memory payload = hex"0a00000000000000";
        uint256 deadline = block.timestamp + 1 hours;

        vm.prank(owner);
        registry.submitJob{value: 1 ether}(JOB_ID, payload, deadline);

        // Warp past the deadline
        vm.warp(deadline + 1);

        bytes memory journal = _buildJournal(JOB_ID, payload, hex"37");

        vm.expectRevert(IJobRegistry.DeadlinePassed.selector);
        vm.prank(worker);
        registry.submitProof(JOB_ID, journal, hex"deadbeef");
    }
}
