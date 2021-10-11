# Astroport Router

The Router Contract contains the logic to facilitate assets multi-hop swap operations via native & Astroport tokens.

**On-chain swap & Astroport is supported.**

README has updated with new messages (Astroport v1 messages follow).

---

Example transactions:
- KRT => UST => mABNB: https://finder.terra.money/tequila-0004/tx/46A1C956D2F4F7A1FA22A8F93749AEADB953ACDFC1B9FB7661EEAB5C59188175
- mABNB => UST => KRT:  https://finder.terra.money/tequila-0004/tx/E9D63CE2C8AC38F6C9434C62F9A8B59F38259FEB86F075D43C253EA485D7F0A9

### Operations Assertion
The contract will check whether the resulting token is swapped into one token.

### Example

Swap KRT => UST => mABNB
```
{
   "execute_swap_operations":{
      "operations":[
         {
            "native_swap":{
               "offer_denom":"ukrw",
               "ask_denom":"uusd"
            }
         },
         {
            "astro_swap":{
               "offer_asset_info":{
                  "native_token":{
                     "denom":"uusd"
                  }
               },
               "ask_asset_info":{
                  "token":{
                     "contract_addr":"terra1avryzxnsn2denq7p2d7ukm6nkck9s0rz2llgnc"
                  }
               }
            }
         }
      ],
      "minimum_receive":"88000"
   }
}
```
