#!/usr/bin/env bash

set -e

projectPath=$(cd "$(dirname "${0}")" && pwd)

if [ ! -d "$projectPath"/astroport-core ];
then
	git clone --branch feat/slack_notification https://github.com/astroport-fi/astroport-core.git "$projectPath"/astroport-core
	if ! (cd "$projectPath"/astroport-core && cargo update && cargo test);
  then
    if [ ! -d "$projectPath"/astroport-core/scripts/node_modules ];
    then
    	cd "$projectPath"/astroport-core/scripts && npm install
    	cp "$projectPath"/.env "$projectPath"/astroport-core/scripts
    fi
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts
  fi
else
  cd "$projectPath"/astroport-core && git fetch && cargo update

  if ! cargo test;
  then
    if [ ! -d "$projectPath"/astroport-core/scripts/node_modules ];
    then
      cd "$projectPath"/astroport-core/scripts && npm install
      cp "$projectPath"/.env "$projectPath"/astroport-core/scripts
    fi
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts
  fi
fi

if [ ! -d "$projectPath"/astroport-governance ];
then
	git clone --branch main https://github.com/astroport-fi/astroport-governance.git "$projectPath"/astroport-governance

	if ! (cd "$projectPath"/astroport-governance && cargo update && cargo test);
  then
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts
  fi
else
   cd "$projectPath"/astroport-governance && git fetch && cargo update

  if ! cargo test;
  then
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts
  fi
fi

if [ ! -d "$projectPath"/astroport-bootstrapping ];
then
	git clone --branch main https://github.com/astroport-fi/astroport-bootstrapping.git "$projectPath"/astroport-bootstrapping
	if ! (cd "$projectPath"/astroport-bootstrapping && cargo update && cargo test);
  then
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts
  fi
else
  cd "$projectPath"/astroport-governance && git fetch && cargo update

  if ! cargo test;
  then
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts
  fi
fi