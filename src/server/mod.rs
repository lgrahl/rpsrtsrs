use std::io::Result as IoResult;
use std::net::{UdpSocket, SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::Duration;
use std::ops::RangeFrom;
use std::collections::HashMap;
use std::f64::consts::PI;

use bincode::{serialize, deserialize, Infinite};

use state::{WorldState, GameState, ClientId, Player, Unit, UnitId};
use network::{Message, Command};

struct Client(ClientId, Receiver<Message>);

/// A `Server` instance holds global server state.
pub struct Server {
    socket_addr: SocketAddr,
    clients: HashMap<ClientId, Sender<Message>>,
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
            clients: HashMap::new(),
            world: world,
            game: game,
            client_id_generator: Arc::new(Mutex::new(0..)),
            unit_id_generator: Arc::new(Mutex::new(0..)),
            unit_targets: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn serve(&mut self) {
        let socket = UdpSocket::bind(self.socket_addr).unwrap();
        println!("Start server: {:?}", socket);

        let game_clone = self.game.clone();
        let unit_targets_clone = self.unit_targets.clone();
        thread::spawn(move || {
            update_world(game_clone, unit_targets_clone);
        });
        let mut packet_buffer = [0; 128];

        // Receive & handle packets
        loop {
            let (_, client_addr) = socket.recv_from(&mut packet_buffer).unwrap();
            let client_message = deserialize(&packet_buffer);
            match client_message {
                Ok(message) => {
                    let new_client = match message {
                        Message::ClientHello => {
                            // Get exclusive world access
                            let mut game_lock = self.game.lock().unwrap();

                            // Create new player for the newly connected client
                            let client_id = self.client_id_generator
                                .lock().expect("Could not lock client_id_generator mutex")
                                .next().expect("No more client IDs available!");
                            let mut player = Player::new(client_id);

                            // Create four initial units for the player
                            let coords = [
                                [50.0f64, 50.0f64], [50.0f64, 100.0f64],
                                [100.0f64, 50.0f64], [100.0f64, 100.0f64],
                            ];
                            for coord in coords.iter() {
                                let unit_id = self.unit_id_generator
                                    .lock().expect("Could not lock unit_id_generator mutex")
                                    .next().expect("No more unit IDs available!");
                                player.units.push(Unit::new(unit_id, *coord));
                            }

                            // Add player to the world
                            let client_id = player.id;
                            game_lock.players.push(player);

                            // Send ServerHello message
                            let encoded: Vec<u8> = serialize(
                                &Message::ServerHello(client_id, self.world.lock().unwrap().clone()),
                                Infinite
                            ).unwrap();
                            socket.send_to(&encoded, client_addr).unwrap();

                            // Create client
                            let (sender, receiver) = channel();
                            self.clients.insert(client_id, sender);
                            Some(Client(client_id, receiver))
                        },
                        Message::ClientReconnect(client_id) => {
                            // Get exclusive world access
                            let world_lock = self.world.lock().unwrap();
                            let game_lock = self.game.lock().unwrap();

                            // Find player with specified id
                            match game_lock.players.iter().find(|player| player.id == client_id) {
                                Some(_) => {
                                    println!("Found you :)");

                                    // Send ServerHello message
                                    let encoded: Vec<u8> = serialize(
                                        &Message::ServerHello(client_id, world_lock.clone()),
                                        Infinite
                                    ).unwrap();
                                    socket.send_to(&encoded, client_addr).unwrap();

                                    // TODO: Drop previous client?

                                    // Create client
                                    let (sender, receiver) = channel();
                                    self.clients.insert(client_id, sender);
                                    Some(Client(client_id, receiver))
                                },
                                None => {
                                    println!("Reconnect to id {} not possible", client_id);

                                    // Send Error message
                                    let encoded: Vec<u8> = serialize(
                                        &Message::Error(client_id),
                                        Infinite).unwrap();
                                    socket.send_to(&encoded, client_addr).unwrap();
                                    None
                                }
                            }
                        },
                        Message::UpdateGameState(client_id, _) |
                        Message::Command(client_id, _) |
                        Message::Error(client_id) => {
                            // Send message to associated client
                            match self.clients.get(&client_id) {
                                Some(sender) => {
                                    sender.send(message).unwrap();
                                },
                                None => {
                                    println!("Client not connected but sent message: {:?}",
                                             message);
                                }
                            };
                            None
                        }
                        Message::ServerHello(_, _) => {
                            println!("Unexpected message type: {:?}", message);
                            None
                        }
                    };

                    // Handle new client
                    match new_client {
                        Some(client) => {
                            let Client(client_id, receiver) = client;

                            // Spawn client thread
                            let socket_clone = socket.try_clone().unwrap();
                            let world_clone = self.world.clone();
                            let game_clone = self.game.clone();
                            let unit_targets = self.unit_targets.clone();
                            println!("Spawning thread...");
                            thread::spawn(move || {
                                handle_client(receiver, socket_clone, client_addr, client_id,
                                              world_clone, game_clone, unit_targets);
                            });
                        },
                        None => {
                            // TODO: Is this the way to ignore None in Rust?
                        }
                    };
                }
                Err(e) => {
                    println!("Invalid packet, error: {:?}", e);
                }
            }
        }
    }
}

pub type SafeWorldState = Arc<Mutex<WorldState>>;
pub type SafeUnitTargets = Arc<Mutex<HashMap<UnitId, [f64; 2]>>>;

pub fn handle_client(receiver: Receiver<Message>,
                     socket: UdpSocket,
                     client_addr: SocketAddr,
                     client_id: ClientId,
                     world: SafeWorldState,
                     game: Arc<Mutex<GameState>>,
                     unit_targets: SafeUnitTargets) {

    let command_socket = socket.try_clone().unwrap();
    let world_clone = world.clone();
    let game_clone = game.clone();
    let unit_targets_clone = unit_targets.clone();
    let client_addr_clone = client_addr.clone();

    // Command receiver loop
    thread::spawn(move || {
        loop {
            let message = receiver.recv().unwrap();
            match message {
                Message::Command(_, command) => {
                    let world_lock = world_clone.lock().unwrap();
                    let mut game_lock = game_clone.lock().unwrap();
                    let mut unit_targets_lock = unit_targets_clone.lock().unwrap();
                    handle_command(&world_lock, &mut game_lock, &mut unit_targets_lock, &command);
                },
                _ => {
                    println!("Did receive unexpected message: {:?}", message);
                    let encoded: Vec<u8> = serialize(&Message::Error(client_id), Infinite).unwrap();
                    command_socket.send_to(&encoded, client_addr_clone).unwrap();
                },
            };
        }
    });

    // GameState loop
    loop {
        let encoded: Vec<u8> = {
            let game_lock = game.lock().unwrap();
            serialize(&*game_lock, Infinite).unwrap()
        };
        match socket.send_to(&encoded, client_addr) {
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
