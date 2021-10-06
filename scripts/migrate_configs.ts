export const testnet: Migrate = {
    contracts: [
        { address: "", filepath: "astroport_factory.wasm", migrate: false},
        { address: "", filepath: "astroport_pair.wasm", migrate: false},
        { address: "", filepath: "astroport_pair_stable.wasm", migrate: false},
        { address: "", filepath: "astroport_router.wasm", migrate: false},
        { address: "", filepath: "astroport_token.wasm", migrate: false},
        { address: "", filepath: "astroport_vesting.wasm", migrate: false},
        { address: "", filepath: "astroport_generator.wasm", migrate: false},
        { address: "", filepath: "astroport_staking.wasm", migrate: false},
        { address: "", filepath: "astroport_maker.wasm", migrate: false}
    ]
}

export const bombay: Migrate = {
    contracts: [
        { address: "", filepath: "astroport_factory.wasm", migrate: false},
        { address: "", filepath: "astroport_pair.wasm", migrate: false},
        { address: "", filepath: "astroport_pair_stable.wasm", migrate: false},
        { address: "", filepath: "astroport_router.wasm", migrate: false},
        { address: "", filepath: "astroport_token.wasm", migrate: false},
        { address: "", filepath: "astroport_vesting.wasm", migrate: false},
        { address: "", filepath: "astroport_generator.wasm", migrate: false},
        { address: "", filepath: "astroport_staking.wasm", migrate: false},
        { address: "", filepath: "astroport_maker.wasm", migrate: false}
    ]
}

export const local: Migrate = {
    contracts: [
        { address: "terra16t7y0vrtpqjw2d7jvc2209yan9002339vjr96d", filepath: "astroport_factory.wasm", migrate: false},
        { address: "", filepath: "astroport_pair.wasm", migrate: false},
        { address: "", filepath: "astroport_pair_stable.wasm", migrate: false},
        { address: "terra1ulgw0td86nvs4wtpsc80thv6xelk76ut7a7apj", filepath: "astroport_router.wasm", migrate: false},
        { address: "terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5", filepath: "astroport_token.wasm", migrate: false},
        { address: "terra1kyl8f2xkd63cga8szgkejdyvxay7mc7qpdc3c5", filepath: "astroport_vesting.wasm", migrate: false},
        { address: "terra1qjrvlf27upqhqnrqmmu2y205ed2c3tc87dnku3", filepath: "astroport_generator.wasm", migrate: true},
        { address: "terra18dt935pdcn2ka6l0syy5gt20wa48n3mktvdvjj", filepath: "astroport_staking.wasm", migrate: true},
        { address: "terra1l09lzlktar3m0hth59z3se86fsyz084map2yln", filepath: "astroport_maker.wasm", migrate: false}
    ]
}