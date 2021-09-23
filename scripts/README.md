## Scripts

### Build

```
./scripts/build_artifacts.sh
./scripts/build_schema.sh
```

### TypeScript and JavaScript scripts

Must be run from the `scripts` directory.

Setup:

```
cd scripts
npm install
```

TypeScript scripts must be executed with `ts-node` using:

```
node --loader ts-node/esm <script>.ts
```

Some scripts require LocalTerra to be running:

```
git clone https://github.com/terra-money/LocalTerra.git
cd LocalTerra
docker-compose up
```

Adjust the `timeout_*` config items in `LocalTerra/config/config.toml` to `250ms` to make the test run faster:

```
sed -E -I .bak '/timeout_(propose|prevote|precommit|commit)/s/[0-9]+m?s/250ms/' config/config.toml
```

### Deploy

```
# build the smart contracts
./scripts/build_artifacts.sh

cd scripts
npm install

# set the deploying wallet
echo "TEST_MAIN=<MNEMONIC_OF_YOUR_DEPLOYING_WALLET>" >> .env

# set the network, defaults to LocalTerra if unset
echo "NETWORK=testnet" >> .env

# ensure the deploy_config.ts has a cw20_code_id specified for above network

node --loader ts-node/esm deploy.ts
```