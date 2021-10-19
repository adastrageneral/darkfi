# DarkFi v0 user tutorial

Welcome to the dark renaissance. This tutorial will teach you how to
install DarkFi on your system, and how to use the testnet to send and
receive anonymous tokens.

This tutorial is intended for standard DarkFi users.  If you'd like to
run a cashier, see this tutorial: [].

DarkFi consists of several software daemons or processes.  These daemons
have seperate, isolated concerns.

As a user, your interest is in the `darkfid` daemon.  This is operated
using the `drk` command-line tool.

# Download

Clone the DarkFi repo:

``` $ git clone https://github.com/darkrenaissance/darkfi

```

# Configure

Now that you have a copy of the software on your device, you will need
to compile the project. But first we must configure our preferences.

DarkFi is highly configurable by design. Key system parameters can be
changed inside the config files.

Default config files can be found here: [example/config](example/config).

First create a new directory for your config files:

``` $ mkdir ~/.config/darkfi ```

Copy darkfid.toml and drk.toml to ~/.config/darkfi.

``` $ cp example/config/darkfid.toml example/config/drk.toml
~/.config/darkfi ```

Take some time to familiarize yourself with the config options.
The defaults should be sufficient for most users and are safe to use
for demo purposes.

See the cashier tutorial [] for how to modify `darkfid.toml` to work
with any cashier.

# Build

Now that DarkFi has been configured we can build the project.

In the project root directory, run the makescript.  This might take some
time if it's your first time building the project.

``` $ make ``` Keep in mind that if you make changes to `darkfid.toml`
or `drk.toml` you will need to run the makescript again for it to
take effect.

# Run

Once the project is compiled you can run the darkfi daemon.

Run `darkfid` in verbose mode:

``` $ ./target/release/darkfid -v ```

Using the command line interface to the `darkfid` daemon, we can make
use of the system:

``` $ ./target/release/drk help

```

# Deposit

Let's start by depositing some coins into DarkFi.

First, we'll need testnet coins on either Bitcoin or Solana.  For Bitcoin
these can be acquired from a faucet like [].  You will need to switch
your Bitcoin wallet to testnet mode.

For Solana, you can either install the Solana command-line suite or
use sollet.io. Follow this tutorial for the Solana command-line [].
For sollet.io, switch the network to testnet and click the ... to airdrop
yourself some testnet coins.

Now that we have testnet coins we can deposit into DarkFi.

We'll do this by sending testnet coins to the DarkFi cashier, which will
issue darkened versions of the deposted coin. This process of darkening
involves the cashier minting new anonymous tokens that are 1:1 redeemable
for deposits.

To deposit testnet BTC:

``` $ ./target/release/drk deposit btc --network bitcoin

```

To deposit testnet SOL:

``` $ ./target/release/drk deposit sol --network solana

```

To deposit any other asset:

``` $ ./target/release/drk deposit [ASSET] --network solana

```

This command will send a deposit request to the cashier.  After running
it, you should get an address printed to your terminal, like this:

[image]

Using Bitcoin or Solana, deposit the desired tokens to the specified
cashier address. This