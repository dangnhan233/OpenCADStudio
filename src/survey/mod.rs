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

    /// Feature classes whose current V1.1 geometry is intentionally a
    /// surveyed POINT only. We do not invent a symbol size or orientation
    /// when the RTK record does not provide one.
    pub fn is_point_feature(&self) -> bool {
        matches!(
            self,
            Self::Point
                | Self::Manhole
                | Self::ElectricPole
                | Self::Culvert
                | Self::Tree
                | Self::Gate
                | Self::Grave
                | Self::Unknown
        )
    }
    /// Returns true only when the measured chain explicitly repeats its
    /// first XYZ coordinate at the end. House geometry never auto-closes.
    pub fn is_explicitly_closed_chain(points: &[SurveyPoint]) -> bool {
        if points.len() < 3 { return false; }
        let first = &points[0];
        let last = &points[points.len() - 1];
        first.x == last.x && first.y == last.y && first.z == last.z
    }
}

/// Configuration for provisional geometric house numbering.
/// start_x/start_y define the beginning; dir_x/dir_y point toward increasing numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HouseNumberingConfig { pub start_x:f64, pub start_y:f64, pub dir_x:f64, pub dir_y:f64 }

#[derive(Clone, Debug, PartialEq)]
pub struct NumberedHouse { pub house_id:usize, pub number:usize }

/// Assign odd numbers to the left side and even numbers to the right side
/// of the configured direction, ordered by projection along that direction.
pub fn assign_house_numbers(houses:&[HouseFeature], cfg:HouseNumberingConfig)->Vec<NumberedHouse>{
    let len=(cfg.dir_x*cfg.dir_x+cfg.dir_y*cfg.dir_y).sqrt();
    if len==0.0 || !len.is_finite(){return Vec::new();}
    let ux=cfg.dir_x/len; let uy=cfg.dir_y/len;
    let mut rows:Vec<(f64,f64,usize)>=houses.iter().map(|h|{let dx=h.centroid_x-cfg.start_x;let dy=h.centroid_y-cfg.start_y;(dx*ux+dy*uy,dx*(-uy)+dy*ux,h.id)}).collect();
    rows.sort_by(|a,b|a.0.total_cmp(&b.0).then_with(||a.1.total_cmp(&b.1)).then_with(||a.2.cmp(&b.2)));
    let mut left=1usize; let mut right=2usize; let mut out=Vec::with_capacity(rows.len());
    for (_,side,id) in rows { if side>=0.0 {out.push(NumberedHouse{house_id:id,number:left});left+=2;} else {out.push(NumberedHouse{house_id:id,number:right});right+=2;} }
    out.sort_by_key(|x|x.house_id); out
}

/// Stable identity and centroid used by SurveyCAD house numbering/export.
#[derive(Clone, Debug, PartialEq)]
pub struct HouseFeature {
    pub id: usize,
    pub point_ids: Vec<usize>,
    pub closed: bool,
    pub centroid_x: f64,
    pub centroid_y: f64,
    pub centroid_z: f64,
}

/// Extract house chains without inventing closure. Non-house records separate houses.
pub fn house_features(import: &SurveyImport) -> Vec<HouseFeature> {
    let mut out = Vec::new();
    let mut chain: Vec<&SurveyPoint> = Vec::new();
    let mut next_id = 1usize;
    let flush = |chain: &mut Vec<&SurveyPoint>, out: &mut Vec<HouseFeature>, next_id: &mut usize| {
        if chain.len() < 2 { chain.clear(); return; }
        let n = chain.len() as f64;
        let cx = chain.iter().map(|p| p.x).sum::<f64>() / n;
        let cy = chain.iter().map(|p| p.y).sum::<f64>() / n;
        let cz = chain.iter().map(|p| p.z).sum::<f64>() / n;
        let closed = FeatureKind::is_explicitly_closed_chain(&chain.iter().map(|p| (*p).clone()).collect::<Vec<_>>());
        out.push(HouseFeature { id:*next_id, point_ids:chain.iter().map(|p| p.id).collect(), closed, centroid_x:cx, centroid_y:cy, centroid_z:cz });
        *next_id += 1;
        chain.clear();
    };
    for p in &import.points {
        if p.feature == FeatureKind::House {
            if chain.last().is_some_and(|q| q.normalized_code == p.normalized_code) { chain.push(p); }
            else { flush(&mut chain, &mut out, &mut next_id); chain.push(p); }
        } else { flush(&mut chain, &mut out, &mut next_id); }
    }
    flush(&mut chain, &mut out, &mut next_id);
    out
}

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
    if p.len() < 4 {
        return Err(SurveyError::InvalidRecord { line, text: p.join(" ") });
    }

    // CODE X Y Z
    if let (Ok(x), Ok(y), Ok(z)) = (p[1].parse(), p[2].parse(), p[3].parse()) {
        return Ok((x, y, z, p[0]));
    }

    // STT X Y Z CODE
    if p.len() >= 5
        && p[0].parse::<usize>().is_ok()
        && let (Ok(x), Ok(y), Ok(z)) = (p[1].parse(), p[2].parse(), p[3].parse())
    {
        return Ok((x, y, z, p[4]));
    }

    // STT CODE X Y Z
    if p.len() >= 5
        && p[0].parse::<usize>().is_ok()
        && let (Ok(x), Ok(y), Ok(z)) = (p[2].parse(), p[3].parse(), p[4].parse())
    {
        return Ok((x, y, z, p[1]));
    }

    Err(SurveyError::InvalidRecord { line, text: p.join(" ") })
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

    #[test]
    fn house_chain_is_split_by_a_different_code() {
        let r = parse_text(
            "NHA 0 0 1\nNHA1 10 0 1\nCNHA 20 0 1\nNHA2 30 0 1\nNHA 40 0 1\nNHA 50 0 1\n"
        ).unwrap();
        let entities = survey_entities(&r);
        let lines = entities.iter().filter(|e| matches!(e, codec::EntityType::Line(_))).count();
        // NHA + NHA1 form one chain; CNHA breaks it; NHA2 + NHA form another.
        assert_eq!(lines, 2);
    }

    #[test]
    fn coordinates_are_preserved_in_line_geometry() {
        let r = parse_text("MDN 1.25 2.5 3.75\nMDN 4.25 5.5 6.75\n").unwrap();
        let entities = survey_entities(&r);
        let line = entities.iter().find_map(|e| match e {
            codec::EntityType::Line(v) => Some(v),
            _ => None,
        }).expect("line");
        assert_eq!(line.start, codec::types::Vector3::new(1.25, 2.5, 3.75));
        assert_eq!(line.end, codec::types::Vector3::new(4.25, 5.5, 6.75));
    }

    #[test]
    fn profile_station_breaks_a_linear_chain() {
        let r = parse_text(
            "MDN 0 0 1\nMDN 10 0 2\n1 20 0 3\nMDN 30 0 4\nMDN 40 0 5\n"
        ).unwrap();
        let entities = survey_entities(&r);
        let lines: Vec<_> = entities.iter().filter(|e| matches!(e, codec::EntityType::Line(_))).collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn unknown_code_breaks_a_linear_chain() {
        let r = parse_text(
            "MDN 0 0 1\nMDN 10 0 2\nZZZ 20 0 3\nMDN 30 0 4\nMDN 40 0 5\n"
        ).unwrap();
        let entities = survey_entities(&r);
        let lines: Vec<_> = entities.iter().filter(|e| matches!(e, codec::EntityType::Line(_))).collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn closed_house_chain_has_closing_segment() {
        let r = parse_text("NHA 0 0 1\\nNHA1 10 0 2\\nNHA2 10 10 3\\nNHA 0 0 1\\n").unwrap();
        let houses: Vec<_> = r.points.iter().filter(|p| p.feature == FeatureKind::House).collect();
        assert!(FeatureKind::is_explicitly_closed_chain(&houses.iter().map(|p| (*p).clone()).collect::<Vec<_>>()));
        let lines = survey_entities(&r).into_iter().filter(|e| matches!(e, codec::EntityType::Line(_))).count();
        assert_eq!(lines, 3);
    }

    #[test]
    fn open_house_chain_is_not_auto_closed() {
        let r = parse_text("NHA 0 0 1\\nNHA1 10 0 2\\nNHA2 10 10 3\\n").unwrap();
        let houses: Vec<SurveyPoint> = r.points.iter().filter(|p| p.feature == FeatureKind::House).cloned().collect();
        assert!(!FeatureKind::is_explicitly_closed_chain(&houses));
        let lines = survey_entities(&r).into_iter().filter(|e| matches!(e, codec::EntityType::Line(_))).count();
        assert_eq!(lines, 2);
    }

    #[test]
    fn house_aliases_share_one_chain() {
        let r = parse_text("NHA 0 0 1\\nNHA1 10 0 2\\nGOCNHA 10 10 3\\nNHA2 0 10 4\\nNHA 0 0 1\\n").unwrap();
        let lines = survey_entities(&r).into_iter().filter(|e| matches!(e, codec::EntityType::Line(_))).count();
        assert_eq!(lines, 4);
    }

    #[test]
    fn gate_breaks_house_chain() {
        let r = parse_text("NHA 0 0 1\\nNHA1 10 0 2\\nCNHA 10 5 3\\nNHA2 10 10 4\\nNHA 0 10 5\\n").unwrap();
        let lines = survey_entities(&r).into_iter().filter(|e| matches!(e, codec::EntityType::Line(_))).count();
        assert_eq!(lines, 2);
    }

    #[test]
    fn house_features_are_stable_and_do_not_auto_close() {
        let r = parse_text("NHA 0 0 1\nNHA1 10 0 2\nNHA2 10 10 3\nCNHA 5 0 4\nNHA 20 0 5\nNHA1 30 0 6\nNHA2 30 10 7\nNHA 20 0 5\n").unwrap();
        let h = house_features(&r);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].id, 1);
        assert!(!h[0].closed);
        assert_eq!(h[0].point_ids, vec![1,2,3]);
        assert!(h[1].closed);
        assert_eq!(h[1].point_ids, vec![5,6,7,8]);
        assert_eq!((h[1].centroid_x, h[1].centroid_y), (25.0, 5.0));
    }

    #[test]
    fn house_numbering_is_deterministic_by_direction_and_side() {
        let houses=vec![
            HouseFeature{id:1,point_ids:vec![],closed:true,centroid_x:10.0,centroid_y:1.0,centroid_z:0.0},
            HouseFeature{id:2,point_ids:vec![],closed:true,centroid_x:20.0,centroid_y:-1.0,centroid_z:0.0},
            HouseFeature{id:3,point_ids:vec![],closed:true,centroid_x:30.0,centroid_y:1.0,centroid_z:0.0},
        ];
        let n=assign_house_numbers(&houses,HouseNumberingConfig{start_x:0.0,start_y:0.0,dir_x:1.0,dir_y:0.0});
        assert_eq!(n,vec![NumberedHouse{house_id:1,number:1},NumberedHouse{house_id:2,number:2},NumberedHouse{house_id:3,number:3}]);
    }

    #[test]
    fn special_features_are_point_only_in_v1_1() {
        for code in ["CAY", "CDI", "HOGA", "CONG", "CONGTRON0,75", "CLANG", "CNHA", "LANG"] {
            let r = parse_text(&format!("{code} 10 20 30\n")).unwrap();
            assert_eq!(r.points[0].is_point_feature(), true, "{code}");
            let entities = survey_entities(&r);
            assert_eq!(entities.len(), 1, "{code} must not invent geometry");
            assert!(matches!(entities[0], codec::EntityType::Point(_)));
        }
    }

}

/// Interactive front-end for the IMPORTSURVEY command.
///
/// The command line owns the file path prompt; actual parsing is delegated to
/// the same parser used by direct IMPORTSURVEY <path>, so GUI and scripted
/// imports cannot diverge.
pub struct SurveyImportCommand;
impl SurveyImportCommand {
    pub fn new() -> Self { Self }
}
impl crate::command::CadCommand for SurveyImportCommand {
    fn name(&self) -> &'static str { "IMPORTSURVEY" }
    fn prompt(&self) -> String {
        "IMPORTSURVEY  Specify TXT/CSV file path:".to_string()
    }
    fn wants_text_input(&self) -> bool { true }
    fn on_text_input(&mut self, text: &str) -> Option<crate::command::CmdResult> {
        let path = text.trim();
        if path.is_empty() { return Some(crate::command::CmdResult::NeedPoint); }
        Some(crate::command::CmdResult::Dispatch(format!("IMPORTSURVEY {path}")))
    }
    fn on_enter(&mut self) -> crate::command::CmdResult {
        crate::command::CmdResult::NeedPoint
    }
    fn on_escape(&mut self) -> crate::command::CmdResult {
        crate::command::CmdResult::Cancel
    }
}

/// Feature-specific native CAD geometry.
///
/// Point features become POINT entities. Linear survey codes become individual
/// LINE entities between consecutive records of the same feature. Using LINE
/// rather than a 2D LWPOLYLINE is intentional: every segment keeps both
/// endpoints' original Z values. Profile stations are never emitted and
/// UNKNOWN codes are never reinterpreted.
pub fn survey_entities(import: &SurveyImport) -> Vec<codec::EntityType> {
    use codec::{Entity, EntityType, Line};
    use codec::types::Vector3;

    // These feature classes are represented as surveyed chains. Chains are
    // split when the canonical code changes, so two nearby houses/walls/etc.
    // cannot accidentally be joined merely because they share a FeatureKind.
    const LINE_FEATURES: &[FeatureKind] = &[
        FeatureKind::RoadEdge,
        FeatureKind::Road,
        FeatureKind::ConcreteRoad,
        FeatureKind::Ditch,
        FeatureKind::Wall,
        FeatureKind::Fence,
        FeatureKind::House,
        FeatureKind::Slope,
        FeatureKind::Field,
        FeatureKind::Bank,
        FeatureKind::FlowerBed,
    ];

    let drawable_count = import
        .points
        .iter()
        .filter(|p| p.feature.is_drawable_point())
        .count();

    let mut out = Vec::with_capacity(drawable_count * 2);

    // Always emit the measured point itself. Profile stations are deliberately
    // not emitted. Non-drawable records are also chain boundaries below.
    for p in import.points.iter().filter(|p| p.feature.is_drawable_point()) {
        let mut point = codec::Point {
            location: Vector3::new(p.x, p.y, p.z),
            ..Default::default()
        };
        point.common.layer = p.feature.layer().to_string();
        out.push(EntityType::Point(point));
    }

    // Build contiguous chains. A chain is allowed to close only when the
    // imported last point is exactly the first point, which avoids inventing
    // a closing segment for open survey strings.
    let mut chain: Vec<&SurveyPoint> = Vec::new();

    let flush = |chain: &mut Vec<&SurveyPoint>, out: &mut Vec<EntityType>| {
        if chain.len() < 2 {
            chain.clear();
            return;
        }

        let feature = chain[0].feature.clone();
        if !LINE_FEATURES.contains(&feature) {
            chain.clear();
            return;
        }

        let layer = feature.layer().to_string();
        for pair in chain.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let mut line = Line::from_points(
                Vector3::new(a.x, a.y, a.z),
                Vector3::new(b.x, b.y, b.z),
            );
            line.set_layer(layer.clone());
            out.push(EntityType::Line(line));
        }

        // Exact repeat of the first measured coordinate means the survey
        // string explicitly closes itself. Do not close merely because the
        // feature is a house/field/etc.
        let first = chain[0];
        let last = chain[chain.len() - 1];
        if chain.len() >= 3
            && FeatureKind::is_explicitly_closed_chain(&chain.iter().map(|p| (*p).clone()).collect::<Vec<_>>())
        {
            // The final repeated point already produced the closing segment
            // in the windows above. No extra zero-length LINE is emitted.
        }

        chain.clear();
    };

    for p in &import.points {
        // Every non-drawable record is a hard boundary. In particular, a
        // PROFILE_STATION between two MDN records must not disappear and
        // accidentally cause those two survey strings to be joined.
        if !p.feature.is_drawable_point() {
            flush(&mut chain, &mut out);
            continue;
        }

        let same_chain = chain.last().is_some_and(|q| {
            q.feature == p.feature && q.normalized_code == p.normalized_code
        });

        if same_chain {
            chain.push(p);
        } else {
            flush(&mut chain, &mut out);
            if LINE_FEATURES.contains(&p.feature) {
                chain.push(p);
            }
        }
    }
    flush(&mut chain, &mut out);

    out
}

/// Backward-compatible point-only helper retained for callers that only need
/// survey points. Profile stations remain excluded.
pub fn point_entities(import: &SurveyImport) -> Vec<codec::EntityType> {
    survey_entities(import)
        .into_iter()
        .filter(|e| matches!(e, codec::EntityType::Point(_)))
        .collect()
}
