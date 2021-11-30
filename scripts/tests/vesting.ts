import {strictEqual} from "assert"
import {
    newClient,
    readArtifact,
    queryContract, Client, toEncodedBinary,
} from "../helpers.js"

async function TestQueryConfig(cl: Client, network: any, network_data: any) {
    let vestingResponse = await queryContract(cl.terra, network.vestingAddress, { config: {} })
    strictEqual(network_data.vesting.initData.owner, vestingResponse.owner)
    strictEqual(network_data.vesting.initData.token_addr, vestingResponse.token_addr)

    console.log('test query vesting config ---> FINISH\n')
}

async function TestQueryVestingAccount(cl: Client, network: any, network_data: any) {
    for ( let i=0; i<network_data.vesting.vesting_accounts.length; i++) {
        try {
            let resp = await queryContract(cl.terra, network.vestingAddress, {
                vesting_account: {
                    address: network_data.vesting.vesting_accounts[i].address
                }
            })

            for ( let j=0; j<network_data.vesting.vesting_accounts[i].schedules.length; j++) {
                strictEqual(toEncodedBinary(network_data.vesting.vesting_accounts[i].schedules[j].start_point), toEncodedBinary(resp.info.schedules[j].start_point))
            }
        } catch (e: any) {
            console.log("Vesting account error for addr: ", network_data.vesting.vesting_accounts[i].address)
            console.log("Error: ", e.response.data)
        }
    }
    console.log('test query vesting account ---> FINISH\n')
}

async function TestQueryVestingAccounts(cl: Client, network: any, network_data: any) {
    for ( let i=0; i<network_data.vesting.vesting_accounts.length; i++) {
        try {
            let resp = await queryContract(cl.terra, network.vestingAddress, {
                vesting_accounts: {}
            })

            if (resp.vesting_accounts.length == network_data.vesting.vesting_accounts.length ) {
                for ( let j=0; j<network_data.vesting.vesting_accounts[i].schedules.length; j++) {
                    strictEqual(network_data.vesting.vesting_accounts[i].address, resp.vesting_accounts[i].address)
                    strictEqual(toEncodedBinary(network_data.vesting.vesting_accounts[i].schedules[j].start_point), toEncodedBinary(resp.vesting_accounts[i].info.schedules[j].start_point))
                }
            } else {
                console.log("Response vesting accounts: ", resp.vesting_accounts)
                console.log("Saved vesting accounts: ", network_data.vesting.vesting_accounts)
            }
        } catch (e: any) {
            console.log("Vesting account error for addr: ", network_data.vesting.vesting_accounts[i].address)
            console.log("Error: ", e)
        }
    }
    console.log('test query vesting accounts ---> FINISH\n')
}

async function TestQueryAvailableAmount(cl: Client, network: any, network_data: any) {
    for ( let i=0; i<network_data.vesting.vesting_accounts.length; i++) {
        try {
            let resp = await queryContract(cl.terra, network.vestingAddress, {
                available_amount: {
                    address: network_data.vesting.vesting_accounts[i].address
                }
            })

            let schedules_total_sum = 0;
            for(let j=0; j<network_data.vesting.vesting_accounts[i].schedules.length; j++) {
                schedules_total_sum += parseFloat(network_data.vesting.vesting_accounts[i].schedules[0].start_point.amount)
            }
            strictEqual(schedules_total_sum.toString(), resp)
        } catch (e: any) {
            console.log("Query available amount error for addr: ", network_data.vesting.vesting_accounts[i].address)
            console.log("Error: ", e)
        }
    }
    console.log('test query vesting available amount ---> FINISH\n')
}

async function main() {
    const client = newClient()
    const network = readArtifact(client.terra.config.chainID)
    const network_data = readArtifact(client.terra.config.chainID + process.env.NETWORK_DATA_ENDING!)
    console.log(`chainID: ${client.terra.config.chainID} wallet: ${client.wallet.key.accAddress}`)

    await TestQueryConfig(client, network, network_data)
    await TestQueryVestingAccount(client, network, network_data)
    await TestQueryVestingAccounts(client, network, network_data)
    await TestQueryAvailableAmount(client, network, network_data)
    console.log('test query message ---> FINISH')
}
main().catch(console.log)
