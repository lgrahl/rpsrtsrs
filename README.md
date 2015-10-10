[![Build Status](https://travis-ci.org/coredump-ch/rpsrtsrs.svg?branch=master)](https://travis-ci.org/coredump-ch/rpsrtsrs)

# rpsrtsrs

Rock-Paper-Scissors-Real-Time-Strategy game written in Rust :wink:

The game is to be played by at least 3 players. Like in Rock-Paper-Scissors,
each player has an advantage against one of the players and a disadvantage
against the other one (`A > B > C > A`).

The user therefore has to decide whether to defend his units against the
stronger user for survival, or whether to attack the weaker user for fame and
glory.

Two units (represented as triangles) can be combined to a building (a square).
In that form, they cannot attack anymore, but have a defense bonus.

![screenshot](docs/images/game-board-initial.png)

## Prequisites

* SDL 2 (`libsdl2-dev` on Ubuntu)
* FreeType (`libfreetype6-dev` on Ubuntu)

## Building

### Linux

Just run

    $ cargo build

To build with the default configuration. If you want to change the window
backend use any of the following:

    $ cargo build --no-default-features --features include_glutin
    $ cargo build --no-default-features --features include_glfw
    $ cargo build --no-default-features --features include_sdl2

### Windows

You will need to install [freetype](https://github.com/PistonDevelopers/freetype-sys#for-windows-users)

## Running

If everything built fine you can use cargo to run the client or the server:

    $ cargo run --bin client
    $ cargo run --bin server

## Ideas

See [ideas](ideas.md)