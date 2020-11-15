pub struct Sede {
    pub name: String,
    pub source: String,
    pub parent_source: String,
    pub codes: Vec<String>,
}

impl Sede {
    pub fn new(name: &str, source: &str, parent_source: &str, code: &str) -> Self {
        Self::supersede(name, source, parent_source, vec![code])
    }
    pub fn supersede(name: &str, source: &str, parent_source: &str, codes: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            source: source.to_string(),
            parent_source: parent_source.to_string(),
            codes: codes.iter().map(|c| c.to_string()).collect(),
        }
    }

    pub fn check_filter_login(url_filter: &Option<Vec<String>>, t: &String) -> bool {
        match url_filter {
            None => true,
            Some(tot) => {
                for f in tot {
                    if t.find(f).is_some() {
                        return true;
                    }
                }
                return false;
            }
        }
    }

    pub fn check_login(&self, t: &String) -> bool {
        for f in &self.codes {
            if t.find(f).is_some() {
                return true;
            }
        }
        return false;
    }
}

pub struct ConfigContest {
    pub sedes: Vec<Sede>,
}

impl ConfigContest {
    pub fn new(sedes: Vec<Sede>) -> Self {
        Self { sedes }
    }

    pub fn get_sede(&self, team: &String) -> Option<String> {
        for sede in &self.sedes {
            if sede.check_login(team) {
                return Some(sede.name.clone());
            }
        }
        None
    }
}
