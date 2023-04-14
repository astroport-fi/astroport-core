import { strictEqual } from "assert";
import { getAccountByName } from "@arufa/wasmkit";

import { AstroportTokenContract } from "../artifacts/typescript_schema/AstroportTokenContract";

function sleep(seconds: number) {
  console.log("Sleeping for " + seconds + " seconds");
  return new Promise(resolve => setTimeout(resolve, seconds * 1000));
}

export default async function run() {
  const runTs = String(new Date());
  const contractOwner = await getAccountByName("admin");

  const astroportToken = new AstroportTokenContract();
  await astroportToken.setupClient();

  const deployResponse = await astroportToken.deploy(
    contractOwner,
  );
  console.log(`Contract ${astroportToken.contractName} deployed: ${deployResponse}`);

  const initMsg = {
    name: "Test 1",
    symbol: "TEST-T",
    decimals: 6,
    initial_balances: [
      {
        address: contractOwner.account.address,
        amount: "1000000000000000"
      }
    ],
    mint: {
      minter: contractOwner.account.address
    }
  };

  const initResponse = await astroportToken.instantiate(
    initMsg,
    `Test astroportToken: ${initMsg.symbol} on timestamp: ${runTs}`,
    contractOwner,
    undefined,
    undefined,
    contractOwner.account.address,
  );
  console.log(`Contract ${astroportToken.contractName} instantiated: ${initResponse}`);

  const balanceResponse = await astroportToken.balance({ address: contractOwner.account.address });
  console.log("balanceResponse: ", balanceResponse);
  strictEqual(balanceResponse.balance, initMsg.initial_balances[0].amount);
}
