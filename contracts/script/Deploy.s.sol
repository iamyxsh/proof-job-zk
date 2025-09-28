// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "../src/JobRegistry.sol";
import "../src/mocks/MockRiscZeroVerifier.sol";

contract DeployScript is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        bytes32 imageId = vm.envBytes32("IMAGE_ID");

        vm.startBroadcast(deployerPrivateKey);

        MockRiscZeroVerifier verifier = new MockRiscZeroVerifier();
        console.log("MockVerifier deployed at:", address(verifier));

        JobRegistry registry = new JobRegistry(address(verifier), imageId);
        console.log("JobRegistry deployed at:", address(registry));

        vm.stopBroadcast();
    }
}
