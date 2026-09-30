// port of: org.apache.commons.jexl3.internal.Scope
// Scopes reference their parent and are mutated through it while parsing (captured variables),
// so they live in an arena (`Scopes`) and refer to each other by index.

/// A scope index into a `Scopes` arena.
pub type ScopeId = usize;

#[derive(Clone, Debug, Default)]
pub struct Scope {
    parent: Option<ScopeId>,
    parms: i32,
    vars: i32,
    /// LinkedHashMap<String, Integer>: insertion ordered; None until first declaration
    named_variables: Option<Vec<(String, i32)>>,
    /// LinkedHashMap<Integer, Integer>: captured symbol -> parent frame symbol
    captured_variables: Option<Vec<(i32, i32)>>,
}

impl PartialEq for Scope {
    // port of: Scope.equals (parameter count and the name->symbol map, order-insensitive)
    fn eq(&self, o: &Self) -> bool {
        if self.parms != o.parms {
            return false;
        }
        match (&self.named_variables, &o.named_variables) {
            (None, None) => true,
            (Some(a), Some(b)) => a.len() == b.len() && a.iter().all(|e| b.contains(e)),
            _ => false,
        }
    }
}

/// The arena of scopes of one parse (and, frozen, of one script).
#[derive(Clone, Debug, Default)]
pub struct Scopes {
    scopes: Vec<Scope>,
}

impl Scopes {
    pub fn new() -> Self {
        Scopes { scopes: Vec::new() }
    }

    // port of: Scope(Scope, String...)
    pub fn create(&mut self, parent: Option<ScopeId>, parameters: Option<&[String]>) -> ScopeId {
        let mut s = Scope { parent, ..Scope::default() };
        if let Some(ps) = parameters {
            s.parms = ps.len() as i32;
            let mut nv: Vec<(String, i32)> = Vec::new();
            for (p, name) in ps.iter().enumerate() {
                // LinkedHashMap.put keeps the first position of a duplicated key
                if let Some(e) = nv.iter_mut().find(|e| &e.0 == name) {
                    e.1 = p as i32;
                } else {
                    nv.push((name.clone(), p as i32));
                }
            }
            s.named_variables = Some(nv);
        }
        self.scopes.push(s);
        self.scopes.len() - 1
    }

    pub fn get(&self, id: ScopeId) -> &Scope {
        &self.scopes[id]
    }

    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    // port of: Scope.getSymbol(String)
    pub fn get_symbol(&mut self, id: ScopeId, name: &str) -> Option<i32> {
        self.get_symbol_capture(id, name, true)
    }

    // port of: Scope.getSymbol(String, boolean)
    fn get_symbol_capture(&mut self, id: ScopeId, name: &str, capture: bool) -> Option<i32> {
        let mut register = self.scopes[id].lookup(name);
        if register.is_none() && capture {
            if let Some(parent) = self.scopes[id].parent {
                if let Some(pr) = self.get_symbol_capture(parent, name, true) {
                    let s = &mut self.scopes[id];
                    let nv = s.named_variables.get_or_insert_with(Vec::new);
                    let r = nv.len() as i32;
                    nv.push((name.to_string(), r));
                    s.captured_variables.get_or_insert_with(Vec::new).push((r, pr));
                    register = Some(r);
                }
            }
        }
        register
    }

    // port of: Scope.declareParameter
    pub fn declare_parameter(&mut self, id: ScopeId, name: &str) -> Result<i32, String> {
        let s = &mut self.scopes[id];
        if s.named_variables.is_none() {
            s.named_variables = Some(Vec::new());
        } else if s.vars > 0 {
            return Err("cant declare parameters after variables".into());
        }
        if let Some(r) = s.lookup(name) {
            return Ok(r);
        }
        let nv = s.named_variables.as_mut().expect("initialized");
        let r = nv.len() as i32;
        nv.push((name.to_string(), r));
        s.parms += 1;
        Ok(r)
    }

    // port of: Scope.declareVariable
    pub fn declare_variable(&mut self, id: ScopeId, name: &str) -> i32 {
        if self.scopes[id].named_variables.is_none() {
            self.scopes[id].named_variables = Some(Vec::new());
        }
        if let Some(r) = self.scopes[id].lookup(name) {
            return r;
        }
        let r = {
            let s = &mut self.scopes[id];
            let nv = s.named_variables.as_mut().expect("initialized");
            let r = nv.len() as i32;
            nv.push((name.to_string(), r));
            s.vars += 1;
            r
        };
        if let Some(parent) = self.scopes[id].parent {
            if let Some(pr) = self.get_symbol_capture(parent, name, true) {
                self.scopes[id].captured_variables.get_or_insert_with(Vec::new).push((r, pr));
            }
        }
        r
    }
}

impl Scope {
    fn lookup(&self, name: &str) -> Option<i32> {
        self.named_variables.as_ref().and_then(|nv| nv.iter().find(|e| e.0 == name).map(|e| e.1))
    }

    pub fn parent(&self) -> Option<ScopeId> {
        self.parent
    }

    // port of: Scope.isCapturedSymbol
    pub fn is_captured_symbol(&self, symbol: i32) -> bool {
        self.captured_variables.as_ref().map(|c| c.iter().any(|e| e.0 == symbol)).unwrap_or(false)
    }

    /// captured symbol -> parent symbol, in declaration order
    pub fn captured_variables(&self) -> &[(i32, i32)] {
        self.captured_variables.as_deref().unwrap_or(&[])
    }

    /// namedVariables.size(), or None when the map was never created
    pub fn named_count(&self) -> Option<usize> {
        self.named_variables.as_ref().map(|v| v.len())
    }

    // port of: Scope.getCaptured
    pub fn get_captured(&self, symbol: i32) -> Option<i32> {
        self.captured_variables.as_ref().and_then(|c| c.iter().find(|e| e.1 == symbol).map(|e| e.0))
    }

    // port of: Scope.getArgCount
    pub fn get_arg_count(&self) -> i32 {
        self.parms
    }

    // port of: Scope.getSymbols
    pub fn get_symbols(&self) -> Vec<String> {
        self.named_variables.as_ref().map(|nv| nv.iter().map(|e| e.0.clone()).collect()).unwrap_or_default()
    }

    // port of: Scope.getParameters
    pub fn get_parameters(&self) -> Vec<String> {
        self.get_parameters_bound(0)
    }

    // port of: Scope.getParameters(int)
    pub fn get_parameters_bound(&self, bound: i32) -> Vec<String> {
        let unbound = self.parms - bound;
        match &self.named_variables {
            Some(nv) if unbound > 0 => nv.iter().filter(|e| e.1 >= bound && e.1 < self.parms).map(|e| e.0.clone()).collect(),
            _ => Vec::new(),
        }
    }

    // port of: Scope.getLocalVariables
    pub fn get_local_variables(&self) -> Vec<String> {
        match &self.named_variables {
            Some(nv) if self.vars > 0 => nv
                .iter()
                .filter(|e| e.1 >= self.parms && !self.is_captured_symbol(e.1))
                .map(|e| e.0.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    // port of: Scope.hashCode
    pub fn java_hash_code(&self) -> i32 {
        match &self.named_variables {
            None => 0,
            Some(nv) => {
                // AbstractMap.hashCode: sum of (key.hashCode ^ value.hashCode)
                let h = nv.iter().fold(0i32, |h, (k, v)| {
                    h.wrapping_add(crate::java::string::JString::from(k.as_str()).hash_code() ^ *v)
                });
                self.parms ^ h
            }
        }
    }
}
