#!/usr/bin/env bash

set -e

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
  if [ ! -d "$projectPath/$1" ];
  then
    git clone --branch main https://github.com/astroport-fi/"$1".git "$projectPath/$1"
    check_node_modules "$1"
    check_multi_test "$1"
  else
    cd "$projectPath/$1" && git pull && cargo update
    check_node_modules "$1"
    check_multi_test "$1"
  fi
}

check_repository "astroport-core"
check_repository "astroport-governance"
check_repository "astroport-bootstrapping"