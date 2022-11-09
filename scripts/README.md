## Astroport Core Scripts

### Build local env

```shell
npm install
npm start
```

### Deploy on `testnet`

Set multisig address in corresponding config or create new one in chain_configs

Build contract:
```shell
npm run build-release
```

Create `.env`:
```shell
WALLET="mnemonic"
LCD_CLIENT_URL=https://pisco-lcd.terra.dev
CHAIN_ID=pisco-1
```

Deploy the contracts:
```shell
npm run build-app
```
