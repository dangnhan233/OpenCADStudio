//! SurveyCAD V1 standalone survey import engine.
//!
//! No MCP dependency. Coordinates are preserved exactly. Unknown codes are
//! never guessed; numeric profile stations 1..15 are excluded from drawing.

use std::{collections::{BTreeMap, BTreeSet}, fmt, path::Path};

pub const MAX_IMPORT_POINTS: usize = 20_000;

#[derive(Clone, Debug, PartialEq)]
pub struct SurveyPoint {
    pub id: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub code: String,
    pub normalized_code: String,
    pub feature: FeatureKind,
    pub source_line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeatureKind {
    Point, RoadEdge, Road, ConcreteRoad, Ditch, Wall, Fence, House, Manhole,
    ElectricPole, Slope, Culvert, Tree, Gate, Grave, Field, Bank, FlowerBed,
    Unknown, ProfileStation,
}

impl FeatureKind {
    pub fn layer(&self) -> &'static str {
        match self {
            Self::Point => "TRAC DIEM",
            Self::RoadEdge | Self::Road | Self::ConcreteRoad => "GIAO THONG",
            Self::Ditch | Self::Bank => "THUY HE",
            Self::Wall | Self::Fence => "TUONG RAO",
            Self::House => "NHA DAN",
            Self::Manhole => "CONG TRINH",
            Self::ElectricPole => "DIEN",
            Self::Slope => "DIA HINH",
            Self::Culvert | Self::Gate => "CONG",
            Self::Tree => "CAY",
            Self::Grave => "MO",
            Self::Field => "RUONG",
            Self::FlowerBed => "CANH QUAN",
            Self::Unknown => "UNKNOWN",
            Self::ProfileStation => "TRAC DOC",
        }
    }
    pub fn is_drawable_point(&self) -> bool {
        !matches!(self, Self::ProfileStation)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurveyImport {
    pub points: Vec<SurveyPoint>,
    pub unknown_codes: BTreeSet<String>,
    pub code_counts: BTreeMap<String, usize>,
}

impl SurveyImport {
    fn from_points(points: Vec<SurveyPoint>) -> Self {
        let mut unknown_codes = BTreeSet::new();
        let mut code_counts = BTreeMap::new();
        for p in &points {
            *code_counts.entry(p.normalized_code.clone()).or_insert(0) += 1;
            if p.feature == FeatureKind::Unknown { unknown_codes.insert(p.code.clone()); }
        }
        Self { points, unknown_codes, code_counts }
    }
    pub fn drawable_points(&self) -> impl Iterator<Item=&SurveyPoint> {
        self.points.iter().filter(|p| p.feature.is_drawable_point())
    }
}

#[derive(Debug)]
pub enum SurveyError {
    Io(std::io::Error),
    TooManyPoints { limit: usize },
    InvalidRecord { line: usize, text: String },
}
impl fmt::Display for SurveyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "survey import failed: {e}"),
            Self::TooManyPoints { limit } => write!(f, "survey import exceeds {limit} points"),
            Self::InvalidRecord { line, text } => write!(f, "invalid survey record at line {line}: {text}"),
        }
    }
}
impl std::error::Error for SurveyError {}
impl From<std::io::Error> for SurveyError { fn from(e: std::io::Error) -> Self { Self::Io(e) } }

pub fn normalize_code(code: &str) -> String {
    code.trim().trim_matches('"').to_uppercase().replace(' ', "").replace('_', "")
}

const RULES: &[(&str, FeatureKind, &[&str])] = &[
    ("MDN", FeatureKind::RoadEdge, &["MOPNHA", "MỘPNHA"]),
    ("DD", FeatureKind::Road, &["DUONGDAT"]),
    ("MBT", FeatureKind::ConcreteRoad, &["MDBT"]),
    ("MGBT", FeatureKind::Ditch, &[]),
    ("HOGA", FeatureKind::Manhole, &[]),
    ("TX", FeatureKind::Wall, &[]),
    ("B40", FeatureKind::Fence, &[]),
    ("NHA", FeatureKind::House, &["GOCNHA","NHA1","NHA2","NH","NHATHO","NHAHOANG","NHAKHONG","NHASAI","NHASHCD"]),
    ("LE", FeatureKind::RoadEdge, &[]),
    ("CDI", FeatureKind::ElectricPole, &["CDT"]),
    ("TALUYDAT2M", FeatureKind::Slope, &[]),
    ("TALUYDA", FeatureKind::Slope, &[]),
    ("CONG", FeatureKind::Culvert, &[]),
    ("CONGTRON0,75", FeatureKind::Culvert, &["CONGTRON075"]),
    ("BONHOA", FeatureKind::FlowerBed, &[]),
    ("CAY", FeatureKind::Tree, &[]),
    ("CLANG", FeatureKind::Gate, &[]),
    ("CNHA", FeatureKind::Gate, &[]),
    ("LANG", FeatureKind::Grave, &[]),
    ("R", FeatureKind::Field, &[]),
    ("BHAO", FeatureKind::Bank, &[]),
    ("Q", FeatureKind::House, &[]),
    ("TD", FeatureKind::Road, &[]),
    ("LONG", FeatureKind::Ditch, &[]),
    ("BOVIA", FeatureKind::Bank, &[]),
    ("MODAT", FeatureKind::Grave, &[]),
    ("MOXAY", FeatureKind::Grave, &[]),
];

pub fn classify_code(code: &str) -> FeatureKind {
    let n = normalize_code(code);
    if n.parse::<u32>().ok().is_some_and(|v| (1..=15).contains(&v)) {
        return FeatureKind::ProfileStation;
    }
    RULES.iter().find(|(c,_,a)| n == *c || a.iter().any(|x| normalize_code(x) == n))
        .map(|(_,f,_)| f.clone()).unwrap_or(FeatureKind::Unknown)
}

pub fn canonical_code(code: &str) -> String {
    let n = normalize_code(code);
    RULES.iter().find(|(c,_,a)| n == *c || a.iter().any(|x| normalize_code(x) == n))
        .map(|(c,_,_)| (*c).to_string()).unwrap_or(n)
}

fn fields(line: &str) -> Vec<&str> {
    if line.contains(',') { line.split(',').map(str::trim).filter(|s|!s.is_empty()).collect() }
    else if line.contains(';') { line.split(';').map(str::trim).filter(|s|!s.is_empty()).collect() }
    else { line.split_whitespace().collect() }
}

fn parse_record(p: &[&str], line: usize) -> Result<(f64,f64,f64,&str),SurveyError> {
    if p.len() < 4 { return Err(SurveyError::InvalidRecord{line,text:p.join(" ")}); }
    if let (Ok(x),Ok(y),Ok(z))=(p[1].parse(),p[2].parse(),p[3].parse()) { return Ok((x,y,z,p[0])); }
    if p.len() >= 5 {
        if let (Ok(x),Ok(y),Ok(z))=(p[1].parse(),p[2].parse(),p[3].parse()) { return Ok((x,y,z,p[4])); }
    }
    Err(SurveyError::InvalidRecord{line,text:p.join(" ")})
}

pub fn parse_text(text: &str) -> Result<SurveyImport,SurveyError> {
    let mut points=Vec::new();
    for (i,raw) in text.lines().enumerate() {
        let line=i+1; let s=raw.trim();
        if s.is_empty() || s.starts_with('#') || s.starts_with("//") { continue; }
        let p=fields(s);
        if p.iter().any(|x| normalize_code(x)=="CODE") { continue; }
        if points.len() >= MAX_IMPORT_POINTS { return Err(SurveyError::TooManyPoints{limit:MAX_IMPORT_POINTS}); }
        let (x,y,z,code)=parse_record(&p,line)?;
        if !(x.is_finite()&&y.is_finite()&&z.is_finite()) { return Err(SurveyError::InvalidRecord{line,text:s.into()}); }
        points.push(SurveyPoint{id:points.len()+1,x,y,z,code:code.into(),normalized_code:canonical_code(code),feature:classify_code(code),source_line:line});
    }
    Ok(SurveyImport::from_points(points))
}

pub fn parse_file(path: impl AsRef<Path>) -> Result<SurveyImport,SurveyError> {
    parse_text(&std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn coordinates_are_unchanged() {
        let r=parse_text("mdn 184000.123456 609000.654321 12.345\n").unwrap();
        let p=&r.points[0]; assert_eq!((p.x,p.y,p.z),(184000.123456,609000.654321,12.345));
    }
    #[test] fn aliases_keep_original_code() {
        let p=&parse_text("MOPNHA 1 2 3\n").unwrap().points[0];
        assert_eq!(p.code,"MOPNHA"); assert_eq!(p.normalized_code,"MDN"); assert_eq!(p.feature,FeatureKind::RoadEdge);
    }
    #[test] fn profile_stations_are_not_drawn() {
        let r=parse_text("1 100 200 10\n15 110 220 11\n").unwrap();
        assert_eq!(r.drawable_points().count(),0);
    }
    #[test] fn unknown_is_not_guessed() {
        assert_eq!(classify_code("ZZZ"),FeatureKind::Unknown);
    }
}