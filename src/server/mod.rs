use std::io::Write;
use std::io::Result as IoResult;
use std::net::{TcpListener, TcpStream, SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::ops::RangeFrom;
use std::collections::HashMap;
use std::f64::consts::PI;

use bincode::{serialize, deserialize_from, Infinite, Bounded};

use state::{WorldState, GameState, Player, Unit, UnitId};
use network::{Message, Command};

/// A `Server` instance holds global server state.
pub struct Server {
    socket_addr: SocketAddr,
    world: Arc<Mutex<WorldState>>,
    game: Arc<Mutex<GameState>>,
    /// Generator that returns sequential unit IDs
    unit_id_generator: Arc<Mutex<RangeFrom<u32>>>,
    /// Generator that returns sequential client IDs
    client_id_generator: Arc<Mutex<RangeFrom<u32>>>,

    /// Map with active unit move commands
    unit_targets: Arc<Mutex<HashMap<UnitId, [f64; 2]>>>,
}

impl Server {
    pub fn new<T: ToSocketAddrs>(addr: T,
                                world_size: (f64, f64))
                                -> IoResult<Server> {
        let addr = try!(addr.to_socket_addrs()).next().unwrap();
        let world = Arc::new(Mutex::new(WorldState::new(world_size.0, world_size.1)));
        let game = Arc::new(Mutex::new(GameState::new()));
        Ok(Server {
            socket_addr: addr,
            world: world,
            game: game,
            client_id_generator: Arc::new(Mutex::new(0..)),
            unit_id_generator: Arc::new(Mutex::new(0..)),
            unit_targets: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn serve(&self) {
        let tcp_listener = TcpListener::bind(self.socket_addr).unwrap();
        println!("Start server: {:?}", tcp_listener);

        let game_clone = self.game.clone();
        let unit_targets_clone = self.unit_targets.clone();
        thread::spawn(move || {
            update_world(game_clone, unit_targets_clone);
        });

        for stream in tcp_listener.incoming() {
            match stream {
                Ok(stream) => {
                    let world_clone = self.world.clone();
                    let game_clone = self.game.clone();
                    let client_id_generator_clone = self.client_id_generator.clone();
                    let unit_id_generator_clone = self.unit_id_generator.clone();
                    let unit_targets = self.unit_targets.clone();
                    println!("Spawning thread...");
                    thread::spawn(move || {
                        handle_client(stream, world_clone, game_clone,
                                      client_id_generator_clone, unit_id_generator_clone, unit_targets);
                    });
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            }
        }
    }
}

pub type SafeWorldState = Arc<Mutex<WorldState>>;
pub type SafeUnitTargets = Arc<Mutex<HashMap<UnitId, [f64; 2]>>>;

pub fn handle_client(mut stream: TcpStream,
                     world: SafeWorldState,
                     game: Arc<Mutex<GameState>>,
                     client_id_generator: Arc<Mutex<RangeFrom<u32>>>,
                     unit_id_generator: Arc<Mutex<RangeFrom<u32>>>,
                     unit_targets: SafeUnitTargets) {

    // handle client hello
    let client_message = deserialize_from(&mut stream, Bounded(128));
    match client_message {
        Ok(message) => {
            match message {
                Message::ClientHello => {
                    // Get exclusive world access
                    let mut game_lock = game.lock().unwrap();

                    // Create new player for the newly connected client
                    let client_id = client_id_generator
                        .lock().expect("Could not lock client_id_generator mutex")
                        .next().expect("No more client IDs available!");
                    let mut player = Player::new(client_id);

                    // Create four initial units for the player
                    let coords = [
                        [50.0f64, 50.0f64], [50.0f64, 100.0f64], [100.0f64, 50.0f64], [100.0f64, 100.0f64],
                    ];
                    for coord in coords.iter() {
                        let unit_id = unit_id_generator
                            .lock().expect("Could not lock unit_id_generator mutex")
                            .next().expect("No more unit IDs available!");
                        player.units.push(Unit::new(unit_id, *coord));
                    }

                    // Add player to the world
                    let player_id = player.id;
                    game_lock.players.push(player);

                    // Send ServerHello message
                    let encoded: Vec<u8> = serialize(
                        &Message::ServerHello(player_id, world.lock().unwrap().clone()),
                        Infinite
                    ).unwrap();
                    stream.write(&encoded).unwrap();
                },
                Message::ClientReconnect(id) => {
                    // Get exclusive world access
                    let world_lock = world.lock().unwrap();
                    let game_lock = game.lock().unwrap();

                    // Find player with specified id
                    match game_lock.players.iter().find(|player| player.id == id) {
                        Some(_) => {
                            println!("Found you :)");

                            // Send ServerHello message
                            let encoded: Vec<u8> = serialize(
                                &Message::ServerHello(id, world_lock.clone()),
                                Infinite
                            ).unwrap();
                            stream.write(&encoded).unwrap();
                        },
                        None => {
                            println!("Reconnect to id {} not possible", id);

                            // Send Error message
                            let encoded: Vec<u8> = serialize(
                                &Message::Error,
                                Infinite).unwrap();
                            stream.write(&encoded).unwrap();
                            return  // Don't enter game loop
                        }
                    }
                },
                _ => {
                    println!("Did not receive ClientHello: {:?}", message);
                    let encoded: Vec<u8> = serialize(&Message::Error, Infinite).unwrap();
                    stream.write(&encoded).unwrap();
                    return  // Don't enter game loop
                }
            }
        }
        Err(e) => {
            println!("Error: {:?}", e);
            return  // Don't enter game loop
        }
    }

    let mut command_stream = stream.try_clone().unwrap();
    let world_clone = world.clone();
    let game_clone = game.clone();
    let unit_targets_clone = unit_targets.clone();
    // Command receiver loop
    thread::spawn(move || {
        loop {
            let client_message = deserialize_from(&mut command_stream, Bounded(128));
            match client_message {
                Ok(message) => {
                    match message {
                        Message::Command(command) => {
                            let world_lock = world_clone.lock().unwrap();
                            let mut game_lock = game_clone.lock().unwrap();
                            let mut unit_targets_lock = unit_targets_clone.lock().unwrap();
                            handle_command(&world_lock, &mut game_lock, &mut unit_targets_lock, &command);
                        },
                        _ => {
                            println!("Did receive unexpected message: {:?}", message);
                            let encoded: Vec<u8> = serialize(&Message::Error, Infinite).unwrap();
                            command_stream.write(&encoded).unwrap();
                            return
                        },
                    }
                },
                Err(e) => {
                    println!("Error: {:?}", e);
                    return;
                }
            };
        }
    });

    // GameState loop
    loop {
        let encoded: Vec<u8> = {
            let game_lock = game.lock().unwrap();
            serialize(&*game_lock, Infinite).unwrap()
        };
        match stream.write(&encoded) {
            Err(e) => {
                println!("Error: {:?}", e);
                return;
            }
            _ => thread::sleep(Duration::from_millis(10)),
        };
    }
}

pub fn handle_command(world: &WorldState,
                      game: &mut GameState,
                      unit_targets: &mut HashMap<UnitId, [f64; 2]>, command: &Command) {
    println!("Did receive command {:?}", command);
    match command {
        &Command::Move(id, move_target) => {
            for player in game.players.iter_mut() {
                for unit in player.units.iter_mut() {
                    if unit.id == id {
                        println!("Found it :)");
                        let mut target = [0.0; 2];
                        target[0] = if move_target[0] > world.x {
                            world.x
                        } else if move_target[0] < 0.0 {
                            0.0
                        } else {
                            move_target[0]
                        };
                        target[1] = if move_target[1] > world.y {
                            world.y
                        } else if move_target[1] < 0.0 {
                            0.0
                        } else {
                            move_target[1]
                        };
                        let dx = target[0] - unit.position[0];
                        let dy = target[1] - unit.position[1];
                        if dx.is_sign_negative() {
                            unit.angle = (dy / dx).atan() + PI;
                        } else {
                            unit.angle = (dy / dx).atan();
                        }
                        unit_targets.insert(id, target);
                    }
                }
            }
            println!("Move {} to {:?}!", id, move_target);
        }
    }
}

pub fn update_world(game: Arc<Mutex<GameState>>, unit_targets: SafeUnitTargets) {
    loop {
        {
            let mut game_lock = game.lock().unwrap();
            let unit_targets = unit_targets.lock().unwrap();
            game_lock.update_targets(&unit_targets);
            game_lock.update(5.0);
        }
        thread::sleep(Duration::from_millis(5));
    }
}
