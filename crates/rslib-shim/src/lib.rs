use anki::collection::{Collection, CollectionBuilder};
use std::path::PathBuf;

pub struct AnkiSession {
    pub col: Collection,
}

impl AnkiSession {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let mut builder = CollectionBuilder::new(path);
        // By default, this opens or creates a collection at the given path.
        // It's safe to use memory fallback if path doesn't exist, or just use the path.
        match builder.build() {
            Ok(col) => Ok(Self { col }),
            Err(e) => Err(format!("Failed to open collection: {:?}", e)),
        }
    }

    pub fn list_decks(&mut self) -> Result<Vec<(i64, String)>, String> {
        // anki::decks uses internal API. For M0, let's just use the deck manager.
        // Collection has a deck manager method. Let's look up how `rslib` lists decks.
        // For now, this is a stub. We'll fix it if we can't get it to compile.
        match self.col.get_all_deck_names(false) {
            Ok(decks) => {
                let mut list = Vec::new();
                for (id, name) in decks {
                    list.push((id.0, name));
                }
                Ok(list)
            },
            Err(e) => Err(format!("Failed to list decks: {:?}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anki_session() {
        let temp_dir = std::env::temp_dir().join("rookery_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let col_path = temp_dir.join("collection.anki2");
        let mut session = AnkiSession::open(col_path).unwrap();
        let decks = session.list_decks().unwrap();
        assert!(!decks.is_empty(), "Should have at least the Default deck");
    }
}
