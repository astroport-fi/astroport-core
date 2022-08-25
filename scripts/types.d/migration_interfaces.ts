type MigrationInfo = {
    address: string,
    name: string,
    message: {}
}

interface Migration {
    contracts: MigrationInfo[]
}