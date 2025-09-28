// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../interfaces/IRiscZeroVerifier.sol";

contract MockRiscZeroVerifier is IRiscZeroVerifier {
    function verify(bytes calldata, bytes32, bytes32) external pure {}
}
