* Given:

  * chain_id: neutron-1

  * the proposal's id that set up pools: 61 (https://app.astroport.fi/governance/proposal/61)

  * the pools receiving ASTRO and their alloc points:

        neutron1vw93hy8tm3xekpz9286428gesmmc8dqxmw8cujsh3fcu3rt0hvdqvlyrrl, 17739
        neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c, 38986
        neutron1kmuv6zmpr2nd3fnqefcffgfmhm74c8vhyerklaphrawyp3398gws74huny, 1754
        neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a, 41521

  * the total alloc point: 100000

  * the ASTRO amount distributed per block, set by the proposal: 1984587

  * the satellite address: neutron1ffus553eet978k024lmssw0czsxwr97mggyv85lpcsdkft8v9ufsz3sa07

  * the generator address: neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny

  * the vesting address: neutron178d2p84ldlzcl53clc25uy6mx3trazxdy08akhjp3qf5chlmccgq6hv2pl


* Querying the block height when the proposal was executed:
  ```
  neutrond q wasm cs smart neutron1ffus553eet978k024lmssw0czsxwr97mggyv85lpcsdkft8v9ufsz3sa07 '{"proposal_state": {"id": 61}}' -o json | jq '.data'

  1437191
  ```

* Checking the pools that had deposits and their last reward blocks before the proposal's execution:
  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_info": {"lp_token": "neutron1vw93hy8tm3xekpz9286428gesmmc8dqxmw8cujsh3fcu3rt0hvdqvlyrrl"}}' --height 1437190 -o json | jq


  pool not found
  ```

  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_info": {"lp_token": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c"}}' --height 1437190 -o json | jq '.data.last_reward_block'

  488583
  ```

  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_info": {"lp_token": "neutron1kmuv6zmpr2nd3fnqefcffgfmhm74c8vhyerklaphrawyp3398gws74huny"}}' --height 1437190 -o json | jq '.data.last_reward_block'

  pool not found
  ```

  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_info": {"lp_token": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a"}}' --height 1437190 -o json | jq '.data.last_reward_block'

  488583
  ```

* Finding the users who had deposites on the pools before the proposal's execution:
  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_stakers": {"lp_token": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c"}}' --height 1437190 -o json | jq '.data'

  [
    {
      "account": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj",
      "amount": "936146544918"
    }
  ]
  ```

  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_stakers": {"lp_token": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a"}}' --height 1437190 -o json | jq '.data'
  [
    {
      "account": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj",
      "amount": "1501110150153"
    }
  ]
  ```

* Checking the user's indexes now (block height 1568424 time 2023-07-18T14:59:48 at the moment of writing this):
  ```
  key: echo "0009$(ascii_to_hex user_info)0042$(ascii_to_hex neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c)$(ascii_to_hex neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj)"

  neutrond q wasm cs raw $generator 0009757365725F696E666F00426E657574726F6E3173783939667879346C7178306E763379733836746B647263683832717967787965633563386478736B3972617A346174357A707134386D3636636E657574726F6E31727968786535667A637A656C63666D72686D63773978326A737179363737667735396673637472303973726B32346C74393365737A776C76796A --height 1568424 -o json | jq -r '.data' | base64 --decode | jq -r '.reward_user_index'

  0

  key: echo "0009$(ascii_to_hex user_info)0042$(ascii_to_hex neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a)$(ascii_to_hex neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj)"

  neutrond q wasm cs raw $generator 0009757365725F696E666F00426E657574726F6E316A6B636638306E6434706663326B72636533786B396D3979393934706C6C713538617678383973667A716C616C656A346672757332376D7333616E657574726F6E31727968786535667A637A656C63666D72686D63773978326A737179363737667735396673637472303973726B32346C74393365737A776C76796A --height 1568424 -o json | jq -r '.data' | base64 --decode | jq -r '.reward_user_index'

  0
  ```

  So, no rewards were withdrawn by the user at the moment

* Checking whether the user will be able to withdraw rewards before the bug fix:
  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pending_token": {"lp_token": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c", "user": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj"}}' --height 1568424

  835429168530
  ```

  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pending_token": {"lp_token": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a", "user": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj"}}' --height 1568424

  889723596667
  ```

  available rewards on the vesting contract for the generator:
    ```
    neutrond q wasm cs raw neutron178d2p84ldlzcl53clc25uy6mx3trazxdy08akhjp3qf5chlmccgq6hv2pl 000C76657374696E675F696E666F6E657574726F6E316A7A3538796A6179387571387A6B667739356E677976336D32776673327A6A65663976647A373564397061343666647478633573787461666E79 -o json --height 1568424 | jq -r '.data' | base64 --decode | jq
    {
      "schedules": [
        {
          "start_point": {
            "time": 1689217200,
            "amount": "0"
          },
          "end_point": {
            "time": 1704942000,
            "amount": "10402412000000"
          }
        }
      ],
      "released_amount": "35347828690"
    }

    (current_time - start_time) / (end_time - start_time) * end_amount - released_amount
    (1689692388 - 1689217200) / (1704942000 - 1689217200) * 10402412000000 - 35347828690 =

    279002837357
    ```

  The minimal unlocked amounts that would allow to withdraw rewards are:
    ```
    835429168530 - 279002837357 = 556426331173
    ```
    or
    ```
    889723596667 - 279002837357 = 610720759310
    ```

  unlocked amount per second by the vesting:
    ```
    10402412000000 / (1704942000 - 1689217200) = 661530
    ```

  So, roughly calculating, the time that we have to fix the bug is the least of the following:

    ```
    insufficient_ASTRO / (vested amount per second * (total_alloc_point - alloc_point) / total_alloc_point) / minutes / hours / days

    556426331173 / (661529 * (100000 - 38986) / 100000) / 60 / 60 / 24 = 15 days

    610720759310 / (661529 * (100000 - 41521) / 100000) / 60 / 60 / 24 = 18 days
    ```
  but the more other generator depositors claim their rewards the more time we have. The best case is that no misscomputed reward will be withdrawn until we fix the bug.

* Querying virtual amounts that the user had on the moment of the proposal execution:
  ```
  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"user_virtual_amount": {"lp_token": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c", "user": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj"}}' --height 1437191

  374458617967

  neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"user_virtual_amount": {"lp_token": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a", "user": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj"}}' --height 1437191

  600444060061
  ```

* The solution to fix misscomputed rewards is to increase the user reward indexes on contract migration as it follows below:
  * query the current user reward indexes for those pools
  * increase them by the following calculated indexes:
      ```
      (setup_pools_block - the wrong last reward block) * tokens_per_block * alloc_point / total_alloc_point / user's virtual amount

      (1437191 - 488583) * 1984587 * 38986 / 100000 / 374458617967 = 1.96002573416386269210

      (1437191 - 488583) * 1984587 * 41521 / 100000 / 600444060061 = 1.30182370931349860257
      ```

* Checking rewards if the fix would apply now:
  * query last reward block and calculate the current pool's global index:
    ```
    neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_info": {"lp_token": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c"}}' --height 1568424 -o json | jq -r '.data.global_reward_index, .data.last_reward_block'

    2.225500979661236903

    1565741

    neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"total_virtual_supply": {"generator": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c"}}' --height 1568424 -o json | jq -r '.data'

    375311882818

    last_global_index + (current_block - last_reward_block) * astro_per_block * alloc_point / total_alloc_point / virtual_supply

    2.225500979661236903 + (1568424 - 1565741) * 1984587 * 38986 / 100000 / 375311882818 = 2.23103202448996594742
    ```

    ```
    neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"pool_info": {"lp_token": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a"}}' --height 1568424 -o json | jq -r '.data.global_reward_index, .data.last_reward_block'

    1.481537641504742878

    1568250

    neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"total_virtual_supply": {"generator": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a"}}' --height 1568424 -o json | jq -r '.data'

    601532655696

    last_global_index + (current_block - last_reward_block) * astro_per_block * alloc_point / total_alloc_point / virtual_supply

    1.481537641504742878 + (1568424 - 1568250) * 1984587 * 41521 / 100000 / 601532655696 = 1.48177599854607936786
    ```

  * query virtual amounts that the user has now:
    ```
    neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"user_virtual_amount": {"lp_token": "neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c", "user": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj"}}' --height 1568424

    374458617967

    neutrond q wasm cs smart neutron1jz58yjay8uq8zkfw95ngyv3m2wfs2zjef9vdz75d9pa46fdtxc5sxtafny '{"user_virtual_amount": {"lp_token": "neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a", "user": "neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj"}}' --height 1568424

    600444060061
    ```

  * calculate reward by indexes:

    (global_reward_index - user_reward_index) * deposit

    ```
    (2.23103202448996594742 - 1.96002573416386269210) * 374458617967 = 101480640935

    (1.48177599854607936786 - 1.30182370931349860257) * 600444060061 = 108051283164
    ```

  * calculate reward by blocks:

    (current_block - proposal_block) * astro_per_block * alloc_point / total_alloc_point

    ```
      (1568424 - 1437191) * 1984587 * 38986 / 100000 = 101536427187

      (1568424 - 1437191) * 1984587 * 41521 / 100000 = 108138664989
    ```

  * rewards by indexes is a bit less, because of rounding issues that occurs by not using those exact types that used in the contract
