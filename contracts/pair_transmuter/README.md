# Astroport Transmuter Pool

## Overview
Implementation of a constant sum pool that supports swapping between tokens at a constant 1:1 ratio.
This pair neither charge any fees nor incur any spread.
Supports pools with >2 tokens.

## Limitations
1. All tokens must have 6 decimals.
2. CW20 tokens are not supported.