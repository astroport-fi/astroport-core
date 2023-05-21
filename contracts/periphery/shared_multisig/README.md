# Astroport Shared Multisig

It is a multisig with two addresses created upon instantiation. Each address has its own role (dao or manager), however, 
both have exactly the same permissions. Each role can propose a new address which can then claim that role.

## Instantiation

To create the multisig, you must pass in a set of address for each one to pass a proposal. To create a 2 multisig, 
pass 2 voters (DAO and Manager).

```json
{
  "max_voting_period": {
    "height": 123
  },
  "manager": "wasm...",
  "dao": "wasm..."
}
```

## ExecuteMsg

### `propose`

Example proposal

```json
{
  "propose": {
     "title": "Example proposal",
     "description": "Example proposal",
     "msgs": [
         {
           "wasm": {
               "execute": {
                   "contract_addr": "wasm...",
                   "msg": "<base64_encoded_json_string>",
                   "funds": []
               }
           }
         }
     ]
  }
}
```

### `vote`

Votes for a proposal with specified parameters

```json
{
  "vote": {
    "proposal_id": 123,
    "vote": {"yes": {}}
  }
}
```

### `execute`

Executes a proposal by ID

```json
{
  "execute": {
    "proposal_id": 123
  }
}
```

### `close`

Closes a proposal by ID

```json
{
  "execute": {
    "proposal_id": 123
  }
}
```

### `update_config`

Updates contract parameters

```json
{
  "update_config": {
    "max_voting_period": 123
  }
}
```

### `propose_new_manager`

Creates an offer to change the contract manager. The validity period of the offer is set in the `expires_in` variable.
After `expires_in` seconds pass, the proposal expires and cannot be accepted anymore.

```json
{
  "propose_new_manager": {
    "manager": "wasm...",
    "expires_in": 1234567
  }
}
```

### `drop_manager_proposal`

Removes an existing offer to change the contract manager.

```json
{
  "drop_manager_proposal": {}
}
```

### `claim_manager`

Used to claim contract manager.

```json
{
  "claim_manager": {}
}
```

### `propose_new_dao`

Creates an offer to change the contract DAO. The validity period of the offer is set in the `expires_in` variable.
After `expires_in` seconds pass, the proposal expires and cannot be accepted anymore.

```json
{
  "propose_new_dao": {
    "dao": "wasm...",
    "expires_in": 1234567
  }
}
```

### `drop_dao_proposal`

Removes an existing offer to change the contract DAO.

```json
{
  "drop_dao_proposal": {}
}
```

### `claim_dao`

Used to claim contract DAO.

```json
{
  "claim_dao": {}
}
```

## QueryMsg

### `config`

Returns the general config of the contract.

```json
{
  "config": {}
}
```

### `proposal`

Returns the information of the proposal

```json
{
  "proposal": { "proposal_id": 123 }
}
```

### `list_proposals`

Returns a list of proposals

```json
{
  "list_proposals": {}
}
```

### `reverse_proposals`

Returns the reversed list of proposals

```json
{
  "reverse_proposals": {}
}
```

### `vote`

Returns the vote (opinion as well as weight counted) as well as the address of the voter who submitted it

```json
{
  "vote": {
    "proposal_id": 123,
    "voter": "wasm..."
  }
}
```

### `list_votes`

Returns a list of votes (opinion as well as weight counted) as well as the addresses of the voters who submitted it

```json
{
  "list_votes": {
    "proposal_id": 123
  }
}
```