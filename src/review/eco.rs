use crate::chess::Game;

use super::openings::opening_book;

/// ECO code prefixes, matched by the longest prefix of the opening name.
/// Codes refer to the ECO class of the line the built-in book names.
const ECO_PREFIXES: &[(&str, &str)] = &[
    ("King's Pawn Opening", "B00"),
    ("Queen's Pawn Opening", "A40"),
    ("English Opening", "A10"),
    ("English Opening: King's English", "A20"),
    ("English Opening: Anglo-Indian Defense", "A15"),
    ("English Opening: Symmetrical Variation", "A30"),
    ("English Opening: Agincourt Defense", "A13"),
    ("Réti Opening", "A09"),
    ("Réti Opening: King's Indian Attack", "A07"),
    ("Zukertort Opening", "A05"),
    ("Bird's Opening", "A02"),
    ("Larsen's Opening", "A01"),
    ("King's Fianchetto Opening", "A00"),
    ("Open Game", "C20"),
    ("King's Knight Opening", "C40"),
    ("King's Knight Opening: Normal Variation", "C44"),
    ("Ruy Lopez", "C60"),
    ("Ruy Lopez: Steinitz Defense", "C62"),
    ("Ruy Lopez: Classical Defense", "C64"),
    ("Ruy Lopez: Berlin Defense", "C65"),
    ("Ruy Lopez: Exchange Variation", "C68"),
    ("Ruy Lopez: Morphy Defense", "C78"),
    ("Ruy Lopez: Open", "C80"),
    ("Ruy Lopez: Closed", "C84"),
    ("Ruy Lopez: Marshall Attack", "C89"),
    ("Italian Game", "C50"),
    ("Italian Game: Evans Gambit", "C51"),
    ("Italian Game: Giuoco Piano", "C53"),
    ("Italian Game: Giuoco Pianissimo", "C54"),
    ("Italian Game: Classical Variation", "C53"),
    ("Italian Game: Two Knights Defense", "C55"),
    ("Italian Game: Two Knights Defense, Fried Liver Attack", "C57"),
    ("Italian Game: Two Knights Defense, Knight Attack", "C57"),
    ("Hungarian Defense", "C50"),
    ("Scotch Game", "C45"),
    ("Scotch Gambit", "C44"),
    ("Four Knights Game", "C46"),
    ("Four Knights Game: Scotch Variation", "C47"),
    ("Four Knights Game: Spanish Variation", "C48"),
    ("Three Knights Opening", "C46"),
    ("Ponziani Opening", "C44"),
    ("Petrov's Defense", "C42"),
    ("Philidor Defense", "C41"),
    ("Latvian Gambit", "C40"),
    ("Elephant Gambit", "C40"),
    ("King's Gambit", "C30"),
    ("King's Gambit Accepted", "C33"),
    ("Vienna Game", "C25"),
    ("Bishop's Opening", "C23"),
    ("Center Game", "C22"),
    ("Danish Gambit", "C21"),
    ("Sicilian Defense", "B20"),
    ("Sicilian Defense: Smith-Morra Gambit", "B21"),
    ("Sicilian Defense: Alapin Variation", "B22"),
    ("Sicilian Defense: Closed", "B23"),
    ("Sicilian Defense: Grand Prix Attack", "B23"),
    ("Sicilian Defense: Old Sicilian", "B30"),
    ("Sicilian Defense: Rossolimo Variation", "B30"),
    ("Sicilian Defense: Open", "B32"),
    ("Sicilian Defense: Sveshnikov Variation", "B33"),
    ("Sicilian Defense: Accelerated Dragon", "B35"),
    ("Sicilian Defense: French Variation", "B40"),
    ("Sicilian Defense: Kan Variation", "B41"),
    ("Sicilian Defense: Four Knights Variation", "B45"),
    ("Sicilian Defense: Taimanov Variation", "B46"),
    ("Sicilian Defense: Modern Variations", "B50"),
    ("Sicilian Defense: Moscow Variation", "B51"),
    ("Sicilian Defense: Classical Variation", "B56"),
    ("Sicilian Defense: Dragon Variation", "B70"),
    ("Sicilian Defense: Scheveningen Variation", "B80"),
    ("Sicilian Defense: Najdorf Variation", "B90"),
    ("French Defense", "C00"),
    ("French Defense: Exchange Variation", "C01"),
    ("French Defense: Advance Variation", "C02"),
    ("French Defense: Tarrasch Variation", "C03"),
    ("French Defense: Rubinstein Variation", "C10"),
    ("French Defense: Paulsen Variation", "C10"),
    ("French Defense: Classical Variation", "C11"),
    ("French Defense: Steinitz Variation", "C11"),
    ("French Defense: Winawer Variation", "C15"),
    ("Caro-Kann Defense", "B10"),
    ("Caro-Kann Defense: Advance Variation", "B12"),
    ("Caro-Kann Defense: Exchange Variation", "B13"),
    ("Caro-Kann Defense: Panov Attack", "B13"),
    ("Caro-Kann Defense: Karpov Variation", "B17"),
    ("Caro-Kann Defense: Classical Variation", "B18"),
    ("Pirc Defense", "B07"),
    ("Pirc Defense: Classical Variation", "B08"),
    ("Pirc Defense: Austrian Attack", "B09"),
    ("Modern Defense", "B06"),
    ("Scandinavian Defense", "B01"),
    ("Alekhine Defense", "B02"),
    ("Alekhine Defense: Four Pawns Attack", "B03"),
    ("Owen Defense", "B00"),
    ("Nimzowitsch Defense", "B00"),
    ("Queen's Pawn Game", "D00"),
    ("Queen's Pawn Game: London System", "D02"),
    ("Queen's Pawn Game: Colle System", "D04"),
    ("Queen's Pawn Game: Torre Attack", "A46"),
    ("Trompowsky Attack", "A45"),
    ("Richter-Veresov Attack", "D01"),
    ("Blackmar-Diemer Gambit", "D00"),
    ("Queen's Gambit", "D06"),
    ("Queen's Gambit Accepted", "D20"),
    ("Queen's Gambit Declined", "D30"),
    ("Queen's Gambit Declined: Tarrasch Defense", "D32"),
    ("Queen's Gambit Declined: Exchange Variation", "D35"),
    ("Queen's Gambit Declined: Harrwitz Attack", "D37"),
    ("Queen's Gambit Declined: Ragozin Defense", "D38"),
    ("Queen's Gambit Declined: Orthodox Defense", "D63"),
    ("Semi-Slav Defense", "D43"),
    ("Semi-Slav Defense: Botvinnik System", "D44"),
    ("Semi-Slav Defense: Meran Variation", "D47"),
    ("Slav Defense", "D10"),
    ("Albin Countergambit", "D08"),
    ("Chigorin Defense", "D07"),
    ("Indian Defense", "A45"),
    ("Indian Defense: Knights Variation", "A46"),
    ("Old Indian Defense", "A53"),
    ("Budapest Gambit", "A51"),
    ("Benko Gambit", "A57"),
    ("Benoni Defense", "A60"),
    ("Benoni Defense: Modern Variation", "A61"),
    ("King's Indian Defense", "E60"),
    ("King's Indian Defense: Fianchetto Variation", "E62"),
    ("King's Indian Defense: Four Pawns Attack", "E76"),
    ("King's Indian Defense: Sämisch Variation", "E80"),
    ("King's Indian Defense: Classical Variation", "E92"),
    ("King's Indian Defense: Orthodox Variation", "E97"),
    ("Grünfeld Defense", "D80"),
    ("Grünfeld Defense: Exchange Variation", "D85"),
    ("Grünfeld Defense: Russian Variation", "D97"),
    ("Nimzo-Indian Defense", "E20"),
    ("Nimzo-Indian Defense: Sämisch Variation", "E24"),
    ("Nimzo-Indian Defense: Classical Variation", "E32"),
    ("Nimzo-Indian Defense: Rubinstein Variation", "E40"),
    ("Queen's Indian Defense", "E12"),
    ("Queen's Indian Defense: Fianchetto Variation", "E15"),
    ("Bogo-Indian Defense", "E11"),
    ("Catalan Opening", "E00"),
    ("Catalan Opening: Open Defense", "E04"),
    ("Catalan Opening: Closed", "E08"),
    ("Dutch Defense", "A80"),
    ("Dutch Defense: Staunton Gambit", "A82"),
    ("Dutch Defense: Stonewall Variation", "A81"),
    ("Dutch Defense: Leningrad Variation", "A87"),
];

/// ECO code for an opening name from the built-in book, using the most
/// specific matching entry.
pub fn eco_code(name: &str) -> Option<&'static str> {
    ECO_PREFIXES
        .iter()
        .filter(|(prefix, _)| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([',', ':', ' ']))
        })
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, code)| *code)
        .or_else(|| {
            let family = name.split(':').next().unwrap_or(name);
            ECO_PREFIXES.iter().find(|(prefix, _)| *prefix == family).map(|(_, code)| *code)
        })
}

/// The opening a game followed: the most specific named book position on the
/// uninterrupted book path from the starting position. `None` for games that
/// start from a custom position or leave the book immediately.
pub fn opening_of(game: &Game) -> Option<(String, Option<String>)> {
    if !game.starts_from_standard_position() {
        return None;
    }
    let book = opening_book();
    let mut found: Option<&'static str> = None;
    for ply in 1..=game.ply_count() {
        let position = game.position_at(ply);
        if !book.contains(position) {
            break;
        }
        if let Some(name) = book.name_of(position) {
            found = Some(name);
        }
    }
    found.map(|name| (name.to_owned(), eco_code(name).map(str::to_owned)))
}

/// Family name of an opening ("Sicilian Defense: Najdorf Variation" -> "Sicilian Defense").
pub fn opening_family(name: &str) -> &str {
    name.split(':').next().unwrap_or(name).trim()
}
