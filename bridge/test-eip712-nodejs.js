const { getEip712TypedData, MsgExecuteContractCompat } = require("@injectivelabs/sdk-ts");
const { EthereumChainId } = require("@injectivelabs/ts-types");

// Create the message
const message = MsgExecuteContractCompat.fromJSON({
  sender: "inj1npvwllfr9dqr8erajqqr6s0vxnk2ak55re90dz",
  contractAddress: "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",
  msg: {
    commit_solution: {
      commitment: "lsKzENeCwdyWWUXEN6zbTwMl3Cg3G7wJJhgne/sJ/N8="
    }
  },
  funds: []
});

// Create EIP-712 typed data
const eip712TypedData = getEip712TypedData({
  msgs: [message],
  tx: {
    accountNumber: "36669",
    sequence: "35849",
    chainId: "injective-888",
    memo: "",
    timeoutHeight: "0",
  },
  fee: {
    amount: [{ denom: "inj", amount: "154585000000000" }],
    gas: "154585"
  },
  ethereumChainId: EthereumChainId.Injective,
});

console.log("=== NODE.JS EIP-712 STRUCTURE ===");
console.log(JSON.stringify(eip712TypedData, null, 2));

// Also log just the message part
console.log("\n=== MESSAGE ONLY ===");
console.log(JSON.stringify(eip712TypedData.message, null, 2));

// And the exact msg field
console.log("\n=== MSG FIELD IN MESSAGE.MSGS[0].VALUE ===");
console.log(eip712TypedData.message.msgs[0].value.msg);
