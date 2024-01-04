**Caution: This is a rough implementation and not ready for production use at all.
There are likely still bugs hiding everywhere.**

# Mercado

A prediction market server based on bitcoin lightning.

This server serves the API Interface for [mercado-ui](https://github.com/HaosGames/mercado-ui) and other clients to use.
It implements the concept described in the [whitepaper](whitepaper.md).
To send and receive bitcoin it connects to a lnbits instance
which needs to be defined in the condif file.
Scripts which I use for running a docker image of lnbits are in the lnbits folder.
There is also a cli to talk to the server api.

## Run Server

```bash
$ nix run .# -- -c ./example.config.json
```

## Run CLI

```bash
$ nix run .#cli -- --help
```

## Build Server

```bash
$ nix build
```

## Enter Development Environment

```bash
$ nix develop
```
