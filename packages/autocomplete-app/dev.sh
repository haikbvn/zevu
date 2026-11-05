#!/usr/bin/env bash

trap 'corepack pnpm remove-host' SIGINT SIGTERM EXIT; { sleep 0.5 && corepack pnpm set-host; } & corepack pnpm exec vite --port 3124