# Shade Agent API Test

This repo is the most simple example of a docker app that can test the Shade Agent API docker image and CLI tool.

## Installation

#### The basics

```
yarn

# install the shade-agent-cli globally
npm i -g @neardefi/shade-agent-cli
```

#### Install near-cli-rs

Follow instructions for your platform here: https://github.com/near/near-cli-rs

#### Create your env file

Create `.env.development.local` in root.

## Env Vars

### Local Development (proxy) or Phala Deployment (sandbox) Contract?

#### Local Development

`NEXT_PUBLIC_contractId=ac-proxy.[YOUR_NEAR_ACCOUNT_ID]` is for local development e.g. yarn dev/start.

You can test against the shade-agent-api running on your machine (NOT A TEE). The contract at `ac-proxy.[YOUR_NEAR_ACCOUNT_ID]` does NOT check any attestation information. Although it does make sure your codehashes are approved and has a somewhat similar interface to the real contract.

#### Phala Deployment

`NEXT_PUBLIC_contractId=ac-sandbox.[YOUR_NEAR_ACCOUNT_ID]` is for remote deployments to Phala Cloud.

These deployments are running in a TEE and the agent contract `ac-sandbox.[YOUR_NEAR_ACCOUNT_ID]` will be checking the attestation of the TEE. This is a full stack shade agent.

### The Full ENV File

```bash
# get this from cargo-near or near-cli-rs
NEAR_ACCOUNT_ID=[YOUR_NEAR_ACCOUNT_ID]
# will need quotes around seed phrase
NEAR_SEED_PHRASE="..."
# ac-proxy is running locally so you can use yarn dev/start and test your app against the locally running shade agent api
NEXT_PUBLIC_contractId=ac-proxy.[YOUR_NEAR_ACCOUNT_ID (from above)]
# including this will give you a fixed account every time, remove for production use as each TEE should boot with a new ephemeral NEAR account ID
ENTROPY=foobar
# do not change the api codehash, this is the shade-agent-api
API_CODEHASH=1952010c00a8b59bc91d9fe3429f45cb97003e67d6dade554dac7231caa65ab5

# FOR PHALA DEPLOYMENTS

# SANDBOX: this will update automatically each time you deploy to Phala with a new image of your app pushed to docker hub
APP_CODEHASH=e25f360bca7ebc6822536d9af2ce926582277e54f37e8c0855bccbf74aac1731

# SANDBOX: what is the docker tag of your app? used for sandbox deployments on Phala
DOCKER_TAG=mattdlockyer/shade-agent-api-test

# SANDBOX: you will need your own Phala API KEY
PHALA_API_KEY=...
```

## Running Locally

**Note:** when you see `Password:` in your terminal, it is asking for your `sudo` password to run docker commands.

```bash
# in Terminal 1
shade-agent-cli
# in Terminal 2
yarn start
```

If everything goes correctly, you should see "deployed to Phala Cloud".

### Test the Endpoints of the Test App

-   http://localhost:3000/api/address
-   http://localhost:3000/api/test-sign

### Test the Endpoints of the Shade Agent API docker image directly

-   http://localhost:3140/api/address
-   http://localhost:3140/api/test-sign

## Running on Phala

Make sure you have your env file filled out with:

```
DOCKER_TAG
PHALA_API_KEY
```

In your ENV file use ac-sanbox for your contract:

`NEXT_PUBLIC_contractId=ac-sandbox.[YOUR_NEAR_ACCOUNT_ID]`

```bash
yarn shade-agent-cli
```
