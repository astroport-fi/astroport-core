const terra_testnet_accounts = [
  {
    name: 'admin',
    address: 'terra1jtdje5vq42sknl22r4wu9sahryu5wcrdztt62s',
    mnemonic: 'category fine rapid trumpet dune early wish under nothing dance property wreck'
  },
];
const terra_mainnet_accounts = [

];

const networks = {
  terra_mainnet: {
    endpoint: 'https://terra-rpc.stakely.io:443/',
    accounts: terra_mainnet_accounts,
    fees: {
      upload: {
        amount: [{ amount: "100000", denom: "uluna" }],
        gas: "500000",
      },
      init: {
        amount: [{ amount: "50000", denom: "uluna" }],
        gas: "250000",
      },
      exec: {
        amount: [{ amount: "50000", denom: "uluna" }],
        gas: "250000",
      }
    },
  },
  terra_testnet: {
    endpoint: 'https://terra-testnet-rpc.polkachu.com:443/',
    accounts: terra_testnet_accounts,
    fees: {
      upload: {
        amount: [{ amount: "100000", denom: "uluna" }],
        gas: "500000",
      },
      init: {
        amount: [{ amount: "50000", denom: "uluna" }],
        gas: "250000",
      },
      exec: {
        amount: [{ amount: "50000", denom: "uluna" }],
        gas: "250000",
      }
    },
  }
};

module.exports = {
  networks: {
    default: networks.terra_testnet,
    testnet: networks.terra_testnet,
    mainnet: networks.terra_mainnet
  },
  mocha: {
    timeout: 6000000
  },
  rust: {
    version: "1.63.0",
  },
  commands: {
    compile: "cargo wasm",
    schema: "cargo schema",
  }
};
