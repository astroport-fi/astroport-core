#!/usr/bin/env bash

set -e

projectPath=$(cd "$(dirname "${0}")" && pwd)

if [ ! -d "$projectPath"/astroport-core ]; 
then
	git clone --branch feat/slack_notification https://github.com/astroport-fi/astroport-core.git "$projectPath"/astroport-core
	cd "$projectPath"/astroport-core/scripts && ./build_release.sh
fi

echo "$projectPath"

if [ ! -d "$projectPath"/astroport-governance ]; 
then
	git clone --branch main https://github.com/astroport-fi/astroport-governance.git "$projectPath"/astroport-governance
	cd "$projectPath"/astroport-governance/scripts && ./build_release.sh
fi

if [ ! -d "$projectPath"/LocalTerra ];
then
	git clone --depth 1 https://github.com/terra-money/LocalTerra.git "$projectPath"/LocalTerra
	sed -E '/timeout_(propose|prevote|precommit|commit)/s/[0-9]+m?s/250ms/' "$projectPath/LocalTerra/config/config.toml" | tee "$projectPath/LocalTerra/config/config.toml"
	cd "$projectPath"/LocalTerra && docker-compose --project-directory "$projectPath"/LocalTerra rm --force --stop && docker-compose --project-directory "$projectPath"/LocalTerra up --detach
else
	sed -E '/timeout_(propose|prevote|precommit|commit)/s/[0-9]+m?s/250ms/' "$projectPath/LocalTerra/config/config.toml" | tee "$projectPath/LocalTerra/config/config.toml"
	cd "$projectPath"/LocalTerra && docker-compose --project-directory "$projectPath"/LocalTerra rm --force --stop && docker-compose --project-directory "$projectPath"/LocalTerra up --detach
fi

if [ ! -d "$projectPath"/astroport-core/scripts/node_modules ];
then
	cd "$projectPath"/astroport-core/scripts && npm install 
	cp "$projectPath"/.env "$projectPath"/astroport-core/scripts
fi

if [ -d "$projectPath"/astroport-core ]; 
then
	cd "$projectPath"/astroport-core/scripts && ./build_app.sh
fi
