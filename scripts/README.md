## Astroport Core Scripts

### Build local env

```shell
npm install
npm start
```

### Deploy on `testnet`

Build contract:
```shell
npm run build-release
```

Create `.env`:
```shell
WALLET="mnemonic"
LCD_CLIENT_URL=https://bombay-lcd.terra.dev
CHAIN_ID=bombay-12
GAS_CURRENCY="uusd"
GAS_PRICE=0.15
```

Deploy the contracts:
```shell
npm run build-app
```
