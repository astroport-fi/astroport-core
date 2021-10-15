## Scripts

### Build env local

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
```

Deploy contract:
```shell
npm run build-app
```
