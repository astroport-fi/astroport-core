## Scripts

### Build local env

```shell
npm install
npm start
```

### Deploy on `testnet`

Build contract:
```shell
npm run build-artifacts
```

Create `.env`:
```shell
WALLET="mnemonic"
LCD_CLIENT_URL=https://bombay-lcd.terra.dev
CHAIN_ID=bombay-12

TOKEN_INITIAL_AMOUNT="1_100_000_000_000000"
```

Deploy contracts:
```shell
npm run build-app
```
