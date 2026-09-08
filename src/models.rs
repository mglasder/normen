#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationKey {
    pub number: i32,
    pub suffix: String,
}

impl CitationKey {
    pub fn new(number: i32) -> Self {
        Self {
            number,
            suffix: String::new(),
        }
    }

    pub fn with_suffix(number: i32, suffix: impl Into<String>) -> Self {
        Self {
            number,
            suffix: suffix.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Norm {
    pub citation: String,
    pub title: String,
    pub text: String,
    pub keys: Vec<CitationKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Law {
    pub abbreviation: String,
    pub title: String,
    pub norms: Vec<Norm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub norm: Norm,
    pub in_title: bool,
    pub preview: String,
    pub score: f64,
}
