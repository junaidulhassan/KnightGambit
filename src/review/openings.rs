use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use shakmaty::fen::Epd;
use shakmaty::san::San;
use shakmaty::{Chess, EnPassantMode, Position};

pub const OPENING_LINES: &[(&str, &str)] = &[
    ("King's Pawn Opening", "e4"),
    ("Queen's Pawn Opening", "d4"),
    ("English Opening", "c4"),
    ("Réti Opening", "Nf3"),
    ("Bird's Opening", "f4"),
    ("Larsen's Opening", "b3"),
    ("King's Fianchetto Opening", "g3"),
    ("Open Game", "e4 e5"),
    ("King's Knight Opening", "e4 e5 Nf3"),
    ("King's Knight Opening: Normal Variation", "e4 e5 Nf3 Nc6"),
    ("Ruy Lopez", "e4 e5 Nf3 Nc6 Bb5"),
    ("Ruy Lopez: Morphy Defense", "e4 e5 Nf3 Nc6 Bb5 a6"),
    ("Ruy Lopez: Exchange Variation", "e4 e5 Nf3 Nc6 Bb5 a6 Bxc6"),
    ("Ruy Lopez: Morphy Defense", "e4 e5 Nf3 Nc6 Bb5 a6 Ba4"),
    ("Ruy Lopez: Morphy Defense", "e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O"),
    ("Ruy Lopez: Closed", "e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Be7 Re1 b5 Bb3 d6 c3 O-O h3"),
    ("Ruy Lopez: Open", "e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Nxe4"),
    ("Ruy Lopez: Marshall Attack", "e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Be7 Re1 b5 Bb3 O-O c3 d5"),
    ("Ruy Lopez: Berlin Defense", "e4 e5 Nf3 Nc6 Bb5 Nf6"),
    ("Ruy Lopez: Berlin Defense", "e4 e5 Nf3 Nc6 Bb5 Nf6 O-O Nxe4 d4 Nd6 Bxc6 dxc6 dxe5 Nf5 Qxd8+ Kxd8"),
    ("Ruy Lopez: Steinitz Defense", "e4 e5 Nf3 Nc6 Bb5 d6"),
    ("Ruy Lopez: Classical Defense", "e4 e5 Nf3 Nc6 Bb5 Bc5"),
    ("Italian Game", "e4 e5 Nf3 Nc6 Bc4"),
    ("Italian Game: Giuoco Piano", "e4 e5 Nf3 Nc6 Bc4 Bc5"),
    ("Italian Game: Giuoco Pianissimo", "e4 e5 Nf3 Nc6 Bc4 Bc5 d3"),
    ("Italian Game: Giuoco Pianissimo", "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6 d3"),
    ("Italian Game: Classical Variation", "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6 d4 exd4 cxd4 Bb4+"),
    ("Italian Game: Evans Gambit", "e4 e5 Nf3 Nc6 Bc4 Bc5 b4"),
    ("Italian Game: Two Knights Defense", "e4 e5 Nf3 Nc6 Bc4 Nf6"),
    ("Italian Game: Two Knights Defense, Fried Liver Attack", "e4 e5 Nf3 Nc6 Bc4 Nf6 Ng5 d5 exd5 Nxd5 Nxf7"),
    ("Italian Game: Two Knights Defense, Knight Attack", "e4 e5 Nf3 Nc6 Bc4 Nf6 Ng5 d5 exd5 Na5"),
    ("Italian Game: Two Knights Defense, Modern Bishop's Opening", "e4 e5 Nf3 Nc6 Bc4 Nf6 d3"),
    ("Hungarian Defense", "e4 e5 Nf3 Nc6 Bc4 Be7"),
    ("Scotch Game", "e4 e5 Nf3 Nc6 d4"),
    ("Scotch Game", "e4 e5 Nf3 Nc6 d4 exd4 Nxd4"),
    ("Scotch Game: Classical Variation", "e4 e5 Nf3 Nc6 d4 exd4 Nxd4 Bc5"),
    ("Scotch Game: Schmidt Variation", "e4 e5 Nf3 Nc6 d4 exd4 Nxd4 Nf6"),
    ("Scotch Gambit", "e4 e5 Nf3 Nc6 d4 exd4 Bc4"),
    ("Four Knights Game", "e4 e5 Nf3 Nc6 Nc3 Nf6"),
    ("Four Knights Game: Spanish Variation", "e4 e5 Nf3 Nc6 Nc3 Nf6 Bb5"),
    ("Four Knights Game: Scotch Variation", "e4 e5 Nf3 Nc6 Nc3 Nf6 d4"),
    ("Three Knights Opening", "e4 e5 Nf3 Nc6 Nc3"),
    ("Ponziani Opening", "e4 e5 Nf3 Nc6 c3"),
    ("Petrov's Defense", "e4 e5 Nf3 Nf6"),
    ("Petrov's Defense: Classical Attack", "e4 e5 Nf3 Nf6 Nxe5 d6 Nf3 Nxe4 d4"),
    ("Petrov's Defense: Three Knights Game", "e4 e5 Nf3 Nf6 Nc3"),
    ("Philidor Defense", "e4 e5 Nf3 d6"),
    ("Philidor Defense", "e4 e5 Nf3 d6 d4 Nf6 Nc3 Nbd7"),
    ("Latvian Gambit", "e4 e5 Nf3 f5"),
    ("Elephant Gambit", "e4 e5 Nf3 d5"),
    ("King's Gambit", "e4 e5 f4"),
    ("King's Gambit Accepted", "e4 e5 f4 exf4"),
    ("King's Gambit Accepted: King's Knight Gambit", "e4 e5 f4 exf4 Nf3"),
    ("King's Gambit Declined: Classical Variation", "e4 e5 f4 Bc5"),
    ("Vienna Game", "e4 e5 Nc3"),
    ("Vienna Game: Falkbeer Variation", "e4 e5 Nc3 Nf6"),
    ("Vienna Game: Vienna Gambit", "e4 e5 Nc3 Nf6 f4"),
    ("Bishop's Opening", "e4 e5 Bc4"),
    ("Bishop's Opening: Berlin Defense", "e4 e5 Bc4 Nf6"),
    ("Center Game", "e4 e5 d4 exd4 Qxd4"),
    ("Danish Gambit", "e4 e5 d4 exd4 c3"),
    ("Sicilian Defense", "e4 c5"),
    ("Sicilian Defense: Open", "e4 c5 Nf3"),
    ("Sicilian Defense: Old Sicilian", "e4 c5 Nf3 Nc6"),
    ("Sicilian Defense: Open", "e4 c5 Nf3 Nc6 d4 cxd4 Nxd4"),
    ("Sicilian Defense: Classical Variation", "e4 c5 Nf3 Nc6 d4 cxd4 Nxd4 Nf6 Nc3 d6"),
    ("Sicilian Defense: Accelerated Dragon", "e4 c5 Nf3 Nc6 d4 cxd4 Nxd4 g6"),
    ("Sicilian Defense: Sveshnikov Variation", "e4 c5 Nf3 Nc6 d4 cxd4 Nxd4 Nf6 Nc3 e5 Ndb5 d6"),
    ("Sicilian Defense: Rossolimo Variation", "e4 c5 Nf3 Nc6 Bb5"),
    ("Sicilian Defense: Modern Variations", "e4 c5 Nf3 d6"),
    ("Sicilian Defense: Open", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3"),
    ("Sicilian Defense: Najdorf Variation", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6"),
    ("Sicilian Defense: Najdorf Variation, English Attack", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Be3 e5 Nb3 Be6 f3"),
    ("Sicilian Defense: Najdorf Variation, Main Line", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Bg5 e6 f4"),
    ("Sicilian Defense: Najdorf Variation, Opocensky Variation", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Be2 e5 Nb3"),
    ("Sicilian Defense: Dragon Variation", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 g6"),
    ("Sicilian Defense: Dragon Variation, Yugoslav Attack", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 g6 Be3 Bg7 f3 O-O Qd2 Nc6"),
    ("Sicilian Defense: Scheveningen Variation", "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 e6"),
    ("Sicilian Defense: Moscow Variation", "e4 c5 Nf3 d6 Bb5+"),
    ("Sicilian Defense: French Variation", "e4 c5 Nf3 e6"),
    ("Sicilian Defense: Taimanov Variation", "e4 c5 Nf3 e6 d4 cxd4 Nxd4 Nc6"),
    ("Sicilian Defense: Kan Variation", "e4 c5 Nf3 e6 d4 cxd4 Nxd4 a6"),
    ("Sicilian Defense: Four Knights Variation", "e4 c5 Nf3 e6 d4 cxd4 Nxd4 Nf6 Nc3 Nc6"),
    ("Sicilian Defense: Closed", "e4 c5 Nc3"),
    ("Sicilian Defense: Closed", "e4 c5 Nc3 Nc6 g3 g6 Bg2 Bg7 d3 d6"),
    ("Sicilian Defense: Alapin Variation", "e4 c5 c3"),
    ("Sicilian Defense: Alapin Variation", "e4 c5 c3 Nf6 e5 Nd5 d4 cxd4"),
    ("Sicilian Defense: Alapin Variation", "e4 c5 c3 d5 exd5 Qxd5 d4"),
    ("Sicilian Defense: Smith-Morra Gambit", "e4 c5 d4 cxd4 c3"),
    ("Sicilian Defense: Grand Prix Attack", "e4 c5 Nc3 Nc6 f4"),
    ("French Defense", "e4 e6"),
    ("French Defense: Normal Variation", "e4 e6 d4 d5"),
    ("French Defense: Exchange Variation", "e4 e6 d4 d5 exd5 exd5"),
    ("French Defense: Advance Variation", "e4 e6 d4 d5 e5"),
    ("French Defense: Advance Variation", "e4 e6 d4 d5 e5 c5 c3 Nc6 Nf3 Qb6"),
    ("French Defense: Tarrasch Variation", "e4 e6 d4 d5 Nd2"),
    ("French Defense: Tarrasch Variation", "e4 e6 d4 d5 Nd2 c5"),
    ("French Defense: Tarrasch Variation", "e4 e6 d4 d5 Nd2 Nf6 e5 Nfd7"),
    ("French Defense: Paulsen Variation", "e4 e6 d4 d5 Nc3"),
    ("French Defense: Winawer Variation", "e4 e6 d4 d5 Nc3 Bb4"),
    ("French Defense: Winawer Variation", "e4 e6 d4 d5 Nc3 Bb4 e5 c5 a3 Bxc3+ bxc3"),
    ("French Defense: Classical Variation", "e4 e6 d4 d5 Nc3 Nf6"),
    ("French Defense: Steinitz Variation", "e4 e6 d4 d5 Nc3 Nf6 e5 Nfd7"),
    ("French Defense: Rubinstein Variation", "e4 e6 d4 d5 Nc3 dxe4"),
    ("Caro-Kann Defense", "e4 c6"),
    ("Caro-Kann Defense", "e4 c6 d4 d5"),
    ("Caro-Kann Defense: Advance Variation", "e4 c6 d4 d5 e5"),
    ("Caro-Kann Defense: Advance Variation", "e4 c6 d4 d5 e5 Bf5"),
    ("Caro-Kann Defense: Exchange Variation", "e4 c6 d4 d5 exd5 cxd5"),
    ("Caro-Kann Defense: Panov Attack", "e4 c6 d4 d5 exd5 cxd5 c4"),
    ("Caro-Kann Defense: Classical Variation", "e4 c6 d4 d5 Nc3 dxe4 Nxe4 Bf5"),
    ("Caro-Kann Defense: Classical Variation", "e4 c6 d4 d5 Nc3 dxe4 Nxe4 Bf5 Ng3 Bg6"),
    ("Caro-Kann Defense: Karpov Variation", "e4 c6 d4 d5 Nc3 dxe4 Nxe4 Nd7"),
    ("Caro-Kann Defense: Two Knights Attack", "e4 c6 Nc3 d5 Nf3"),
    ("Pirc Defense", "e4 d6 d4 Nf6 Nc3 g6"),
    ("Pirc Defense: Austrian Attack", "e4 d6 d4 Nf6 Nc3 g6 f4"),
    ("Pirc Defense: Classical Variation", "e4 d6 d4 Nf6 Nc3 g6 Nf3 Bg7 Be2"),
    ("Modern Defense", "e4 g6"),
    ("Modern Defense", "e4 g6 d4 Bg7"),
    ("Scandinavian Defense", "e4 d5"),
    ("Scandinavian Defense: Main Line", "e4 d5 exd5 Qxd5 Nc3 Qa5"),
    ("Scandinavian Defense: Valencian Variation", "e4 d5 exd5 Qxd5 Nc3 Qd6"),
    ("Scandinavian Defense: Mieses-Kotroc Variation", "e4 d5 exd5 Qxd5 Nc3 Qd8"),
    ("Scandinavian Defense: Modern Variation", "e4 d5 exd5 Nf6"),
    ("Alekhine Defense", "e4 Nf6"),
    ("Alekhine Defense: Modern Variation", "e4 Nf6 e5 Nd5 d4 d6 Nf3"),
    ("Alekhine Defense: Four Pawns Attack", "e4 Nf6 e5 Nd5 d4 d6 c4 Nb6 f4"),
    ("Owen Defense", "e4 b6"),
    ("Nimzowitsch Defense", "e4 Nc6"),
    ("Queen's Pawn Game", "d4 d5"),
    ("Queen's Pawn Game", "d4 d5 Nf3"),
    ("Queen's Pawn Game: London System", "d4 d5 Bf4"),
    ("Queen's Pawn Game: London System", "d4 d5 Nf3 Nf6 Bf4"),
    ("Queen's Pawn Game: London System", "d4 Nf6 Bf4"),
    ("Queen's Pawn Game: London System", "d4 d5 Bf4 Nf6 e3 c5 c3 Nc6 Nd2"),
    ("Queen's Pawn Game: Colle System", "d4 d5 Nf3 Nf6 e3"),
    ("Queen's Pawn Game: Jobava London", "d4 d5 Nc3 Nf6 Bf4"),
    ("Queen's Pawn Game: Torre Attack", "d4 Nf6 Nf3 e6 Bg5"),
    ("Trompowsky Attack", "d4 Nf6 Bg5"),
    ("Richter-Veresov Attack", "d4 Nf6 Nc3 d5 Bg5"),
    ("Blackmar-Diemer Gambit", "d4 d5 e4 dxe4 Nc3"),
    ("Queen's Gambit", "d4 d5 c4"),
    ("Queen's Gambit Accepted", "d4 d5 c4 dxc4"),
    ("Queen's Gambit Accepted: Main Line", "d4 d5 c4 dxc4 Nf3 Nf6 e3 e6 Bxc4 c5"),
    ("Queen's Gambit Declined", "d4 d5 c4 e6"),
    ("Queen's Gambit Declined", "d4 d5 c4 e6 Nc3 Nf6"),
    ("Queen's Gambit Declined: Orthodox Defense", "d4 d5 c4 e6 Nc3 Nf6 Bg5 Be7 e3 O-O Nf3 Nbd7"),
    ("Queen's Gambit Declined: Exchange Variation", "d4 d5 c4 e6 Nc3 Nf6 cxd5 exd5"),
    ("Queen's Gambit Declined: Ragozin Defense", "d4 d5 c4 e6 Nc3 Nf6 Nf3 Bb4"),
    ("Queen's Gambit Declined: Harrwitz Attack", "d4 d5 c4 e6 Nc3 Nf6 Nf3 Be7 Bf4"),
    ("Queen's Gambit Declined: Tarrasch Defense", "d4 d5 c4 e6 Nc3 c5"),
    ("Semi-Slav Defense", "d4 d5 c4 e6 Nc3 Nf6 Nf3 c6"),
    ("Semi-Slav Defense: Meran Variation", "d4 d5 c4 e6 Nc3 Nf6 Nf3 c6 e3 Nbd7 Bd3 dxc4 Bxc4 b5"),
    ("Semi-Slav Defense: Botvinnik System", "d4 d5 c4 e6 Nc3 Nf6 Nf3 c6 Bg5 dxc4"),
    ("Slav Defense", "d4 d5 c4 c6"),
    ("Slav Defense", "d4 d5 c4 c6 Nf3 Nf6 Nc3"),
    ("Slav Defense: Main Line", "d4 d5 c4 c6 Nf3 Nf6 Nc3 dxc4 a4 Bf5"),
    ("Slav Defense: Exchange Variation", "d4 d5 c4 c6 cxd5 cxd5"),
    ("Albin Countergambit", "d4 d5 c4 e5"),
    ("Chigorin Defense", "d4 d5 c4 Nc6"),
    ("Indian Defense", "d4 Nf6"),
    ("Indian Defense", "d4 Nf6 c4"),
    ("Indian Defense: Knights Variation", "d4 Nf6 Nf3"),
    ("King's Indian Defense", "d4 Nf6 c4 g6"),
    ("King's Indian Defense", "d4 Nf6 c4 g6 Nc3 Bg7 e4 d6"),
    ("King's Indian Defense: Classical Variation", "d4 Nf6 c4 g6 Nc3 Bg7 e4 d6 Nf3 O-O Be2 e5"),
    ("King's Indian Defense: Orthodox Variation", "d4 Nf6 c4 g6 Nc3 Bg7 e4 d6 Nf3 O-O Be2 e5 O-O Nc6 d5 Ne7"),
    ("King's Indian Defense: Sämisch Variation", "d4 Nf6 c4 g6 Nc3 Bg7 e4 d6 f3"),
    ("King's Indian Defense: Four Pawns Attack", "d4 Nf6 c4 g6 Nc3 Bg7 e4 d6 f4"),
    ("King's Indian Defense: Fianchetto Variation", "d4 Nf6 c4 g6 Nf3 Bg7 g3 O-O Bg2 d6 O-O"),
    ("Grünfeld Defense", "d4 Nf6 c4 g6 Nc3 d5"),
    ("Grünfeld Defense: Exchange Variation", "d4 Nf6 c4 g6 Nc3 d5 cxd5 Nxd5 e4 Nxc3 bxc3 Bg7"),
    ("Grünfeld Defense: Russian Variation", "d4 Nf6 c4 g6 Nc3 d5 Nf3 Bg7 Qb3"),
    ("Nimzo-Indian Defense", "d4 Nf6 c4 e6 Nc3 Bb4"),
    ("Nimzo-Indian Defense: Classical Variation", "d4 Nf6 c4 e6 Nc3 Bb4 Qc2"),
    ("Nimzo-Indian Defense: Rubinstein Variation", "d4 Nf6 c4 e6 Nc3 Bb4 e3"),
    ("Nimzo-Indian Defense: Sämisch Variation", "d4 Nf6 c4 e6 Nc3 Bb4 a3 Bxc3+ bxc3"),
    ("Queen's Indian Defense", "d4 Nf6 c4 e6 Nf3 b6"),
    ("Queen's Indian Defense: Fianchetto Variation", "d4 Nf6 c4 e6 Nf3 b6 g3 Ba6"),
    ("Bogo-Indian Defense", "d4 Nf6 c4 e6 Nf3 Bb4+"),
    ("Catalan Opening", "d4 Nf6 c4 e6 g3"),
    ("Catalan Opening: Open Defense", "d4 Nf6 c4 e6 g3 d5 Bg2 dxc4"),
    ("Catalan Opening: Closed", "d4 Nf6 c4 e6 g3 d5 Bg2 Be7 Nf3 O-O O-O"),
    ("Benoni Defense", "d4 Nf6 c4 c5 d5"),
    ("Benoni Defense: Modern Variation", "d4 Nf6 c4 c5 d5 e6 Nc3 exd5 cxd5 d6"),
    ("Benko Gambit", "d4 Nf6 c4 c5 d5 b5"),
    ("Old Indian Defense", "d4 Nf6 c4 d6"),
    ("Budapest Gambit", "d4 Nf6 c4 e5"),
    ("Dutch Defense", "d4 f5"),
    ("Dutch Defense: Leningrad Variation", "d4 f5 c4 Nf6 g3 g6 Bg2 Bg7"),
    ("Dutch Defense: Stonewall Variation", "d4 f5 c4 Nf6 g3 e6 Bg2 d5"),
    ("Dutch Defense: Staunton Gambit", "d4 f5 e4"),
    ("English Opening: King's English Variation", "c4 e5"),
    ("English Opening: King's English, Reversed Sicilian", "c4 e5 Nc3 Nf6 g3 d5 cxd5 Nxd5"),
    ("English Opening: Anglo-Indian Defense", "c4 Nf6"),
    ("English Opening: Symmetrical Variation", "c4 c5"),
    ("English Opening: Symmetrical Variation", "c4 c5 Nc3 Nc6 g3 g6 Bg2 Bg7"),
    ("English Opening: Agincourt Defense", "c4 e6"),
    ("Réti Opening", "Nf3 d5 c4"),
    ("Réti Opening: King's Indian Attack", "Nf3 d5 g3"),
    ("Réti Opening: King's Indian Attack", "Nf3 Nf6 g3 g6 Bg2 Bg7 O-O O-O d3"),
    ("Zukertort Opening", "Nf3 Nf6"),
];

#[derive(Debug, Default)]
pub struct OpeningBook {
    positions: HashSet<String>,
    names: HashMap<String, &'static str>,
}

pub fn position_key(position: &Chess) -> String {
    Epd::from_position(position, EnPassantMode::Legal).to_string()
}

impl OpeningBook {
    pub fn build() -> Self {
        let mut book = OpeningBook::default();
        for (name, line) in OPENING_LINES {
            let mut position = Chess::default();
            for san in line.split_whitespace() {
                let Ok(parsed) = san.parse::<San>() else { break };
                let Ok(mv) = parsed.to_move(&position) else { break };
                position.play_unchecked(mv);
                book.positions.insert(position_key(&position));
            }
            book.names.insert(position_key(&position), name);
        }
        book
    }

    pub fn contains(&self, position: &Chess) -> bool {
        self.positions.contains(&position_key(position))
    }

    pub fn name_of(&self, position: &Chess) -> Option<&'static str> {
        self.names.get(&position_key(position)).copied()
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

pub fn opening_book() -> &'static OpeningBook {
    static BOOK: OnceLock<OpeningBook> = OnceLock::new();
    BOOK.get_or_init(OpeningBook::build)
}

pub fn invalid_lines() -> Vec<(&'static str, String)> {
    let mut invalid = Vec::new();
    for (name, line) in OPENING_LINES {
        let mut position = Chess::default();
        for san in line.split_whitespace() {
            match san.parse::<San>().ok().and_then(|parsed| parsed.to_move(&position).ok()) {
                Some(mv) => position.play_unchecked(mv),
                None => {
                    invalid.push((*name, format!("{line} (at {san})")));
                    break;
                }
            }
        }
    }
    invalid
}
