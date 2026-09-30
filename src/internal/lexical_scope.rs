// port of: org.apache.commons.jexl3.internal.LexicalScope
// The Java class uses a long for the first 64 symbols and a BitSet beyond; a growable word
// vector is observably identical.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LexicalScope {
    words: Vec<u64>,
}

impl LexicalScope {
    pub fn new() -> Self {
        LexicalScope { words: Vec::new() }
    }

    // port of: LexicalScope.hasSymbol
    pub fn has_symbol(&self, symbol: i32) -> bool {
        if symbol < 0 {
            return false;
        }
        let (w, b) = (symbol as usize / 64, symbol as usize % 64);
        self.words.get(w).map(|x| x & (1u64 << b) != 0).unwrap_or(false)
    }

    // port of: LexicalScope.addSymbol
    pub fn add_symbol(&mut self, symbol: i32) -> bool {
        let (w, b) = (symbol as usize / 64, symbol as usize % 64);
        if self.words.len() <= w {
            self.words.resize(w + 1, 0);
        }
        if self.words[w] & (1u64 << b) != 0 {
            return false;
        }
        self.words[w] |= 1u64 << b;
        true
    }

    // port of: LexicalScope.clearSymbols (in ascending symbol order, like the Java loops)
    pub fn clear_symbols(&mut self, mut clean_symbol: impl FnMut(i32)) {
        for (w, word) in self.words.iter().enumerate() {
            let mut clean = *word;
            while clean != 0 {
                let s = clean.trailing_zeros() as usize;
                clean &= !(1u64 << s);
                clean_symbol((w * 64 + s) as i32);
            }
        }
        self.words.clear();
    }

    // port of: LexicalScope.getSymbolCount
    pub fn get_symbol_count(&self) -> i32 {
        self.words.iter().map(|w| w.count_ones() as i32).sum()
    }

    /// Symbols in ascending order.
    pub fn symbols(&self) -> Vec<i32> {
        let mut v = Vec::new();
        for (w, word) in self.words.iter().enumerate() {
            for b in 0..64 {
                if word & (1u64 << b) != 0 {
                    v.push((w * 64 + b) as i32);
                }
            }
        }
        v
    }
}
