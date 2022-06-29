#!/usr/bin/env bash

set -o pipefail

projectPath=$(cd "$(dirname "${0}")" && pwd)

function check_node_modules {
   if [ ! -d "$projectPath/$1/scripts/node_modules" ];
   then
     cd "$projectPath/$1/scripts" && npm install
     cp "$projectPath"/.env "$projectPath/$1/scripts"
   fi
}

function check_multi_test {
  cd "$projectPath/$1" && cargo update

  if ! ERROR=$(cargo test);
  then
    cd "$projectPath"/astroport-core/scripts && node --loader ts-node/esm slack_notification.ts "$ERROR"
  fi
}

function check_repository {
  for REPOSITORY in "$@"
  do
    if [ ! -d "$projectPath/$REPOSITORY" ];
    then
      git clone --branch main https://github.com/astroport-fi/"$REPOSITORY".git "$projectPath/$REPOSITORY"
      check_node_modules "$REPOSITORY"
      check_multi_test "$REPOSITORY"
    else
      cd "$projectPath/$REPOSITORY" && git pull && cargo update
      check_node_modules "$REPOSITORY"
      check_multi_test "$REPOSITORY"
    fi
  done
}

check_repository "astroport-core" "astroport-governance" "astroport-bootstrapping"