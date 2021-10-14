# Deploying a contracts at the Terra Station. #

## Bootstrap verifier
* This demo uses `package.json` to bootstrap all dependencies.
  ```shell
  $ cp sample.local.env .env
  $ npm install
  $ npm start
  ```

### Overview a scripts
* This script compiles all contracts into files with the .wasm extension.
  ```shell
  npm run build-artifacts
  ```

* This script deploys all contracts to exists a TerraStation environment. 
  ```shell
  npm run build-app
  ```

### Output Result
* As a result, we will get a data file `<chain-id>.json` located in the root folder by default.
  ```json
  {
    "factory": {
      "ID": 176,
      "Addr": "terra1njg0ed835rzt2ee9yw0ek0kezadzv5zzqrwad6"
    },
    "generator": {
      "ID": 177,
      "Addr": "terra1dqaeaxhlslxnsf8yuz2leeu3yuu0zzq3usk8h4"
    },
    "mirror": {
      "ID": 178
    },
    "maker": {
      "ID": 179,
      "Addr": "terra1hznlcgwtyrqg7sq0qurffy7a4zdd8gahdak4z3"
    },
    "oracle": {
      "ID": 180
    },
    "pair": {
      "ID": 181
    },
    "stable": {
      "ID": 182
    },
    "router": {
      "ID": 183,
      "Addr": "terra12nz0lf4sg8lu8agxej8tjfmmecy5hy562kp9h4"
    },
    "staking": {
      "ID": 184,
      "Addr": "terra1ky05d9cylg2scl0xf5h76je3jvyxdc08cw564c"
    },
    "token": {
      "ID": 185,
      "Addr": "terra1tv73ust2prgnp9njmzhy0g94sly2y5956ttm3m"
    },
    "vesting": {
      "ID": 186,
      "Addr": "terra1syr57umnt2q9k720ey3x3ph43s3n5vl7ns9r5y"
    }
  }
  ```