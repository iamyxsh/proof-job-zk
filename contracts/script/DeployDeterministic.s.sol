// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "../src/JobRegistry.sol";
import "../src/mocks/MockRiscZeroVerifier.sol";

contract DeployDeterministicScript is Script {
    bytes32 constant VERIFIER_SALT = keccak256("proof-job-zk.MockRiscZeroVerifier.v1");
    bytes32 constant REGISTRY_SALT = keccak256("proof-job-zk.JobRegistry.v1");

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        bytes32 imageId = vm.envBytes32("IMAGE_ID");

        address expectedVerifier = vm.computeCreate2Address(
            VERIFIER_SALT,
            hashInitCode(type(MockRiscZeroVerifier).creationCode)
        );
        address expectedRegistry = vm.computeCreate2Address(
            REGISTRY_SALT,
            hashInitCode(type(JobRegistry).creationCode, abi.encode(expectedVerifier, imageId))
        );

        console.log("Expected MockVerifier:", expectedVerifier);
        console.log("Expected JobRegistry:", expectedRegistry);

        vm.startBroadcast(deployerPrivateKey);

        MockRiscZeroVerifier verifier = new MockRiscZeroVerifier{salt: VERIFIER_SALT}();
        require(address(verifier) == expectedVerifier, "verifier address mismatch");
        console.log("MockVerifier deployed at:", address(verifier));

        JobRegistry registry = new JobRegistry{salt: REGISTRY_SALT}(address(verifier), imageId);
        require(address(registry) == expectedRegistry, "registry address mismatch");
        console.log("JobRegistry deployed at:", address(registry));

        vm.stopBroadcast();
    }

    function predict() external view {
        bytes32 imageId = vm.envBytes32("IMAGE_ID");

        address expectedVerifier = vm.computeCreate2Address(
            VERIFIER_SALT,
            hashInitCode(type(MockRiscZeroVerifier).creationCode)
        );
        address expectedRegistry = vm.computeCreate2Address(
            REGISTRY_SALT,
            hashInitCode(type(JobRegistry).creationCode, abi.encode(expectedVerifier, imageId))
        );

        console.log("MockVerifier:", expectedVerifier);
        console.log("JobRegistry:", expectedRegistry);
    }
}
