import {execSync} from 'child_process'
import codegen from '@cosmwasm/ts-codegen';
import * as fs from "fs";

interface Entry {
    name: string,
    example: string
}

function read_json(): Entry[] {
    let input = fs.readFileSync(process.stdin.fd, 'utf-8')
    return JSON.parse(input)
}

function main() {
    let schemes = read_json().map((contract) => {
        let schema_path = `schemes/${contract.name}`
        execSync(`mkdir -p ${schema_path}`)
        execSync(`cd ${schema_path} && cargo run -p ${contract.name} --example ${contract.example}`);
        return {
            name: contract.name,
            dir: schema_path
        }
    });
    codegen({
        contracts: schemes,
        outPath: "./src",
        options: {
            bundle: {
                bundleFile: "index.ts"
            },
            types: {
                enabled: true,
            },
            client: {
                enabled: true,
            },
            messageComposer: {
                enabled: true,
            },
        }
    }).then(() => {
        console.log('✨ all done!');
    });
}

main()