use super::*;

impl OpenCADStudio {
    pub(super) fn dispatch_survey(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {

        if let Some(args) = cmd.strip_prefix("NUMBERHOUSES").map(str::trim) {
            // Accept quoted paths so survey files can live in directories with spaces.
            let (path_text, direction_args) = if let Some(quoted) = args.strip_prefix('"') {
                match quoted.find('"') {
                    Some(end) => (&quoted[..end], quoted[end + 1..].split_whitespace().collect::<Vec<_>>()),
                    None => {
                        self.command_line.push_error("NUMBERHOUSES: missing closing quote around survey path.");
                        return Some(Task::none());
                    }
                }
            } else {
                let mut split = args.split_whitespace();
                let Some(path) = split.next() else {
                    self.command_line.push_error("NUMBERHOUSES: specify survey TXT/CSV path and optional direction DX DY.");
                    return Some(Task::none());
                };
                (path, split.collect::<Vec<_>>())
            };
            if path_text.is_empty() {
                self.command_line.push_error("NUMBERHOUSES: specify a non-empty survey TXT/CSV path.");
                return Some(Task::none());
            }
            if direction_args.len() > 2 {
                self.command_line.push_error("NUMBERHOUSES: expected a survey path and at most two direction values DX DY.");
                return Some(Task::none());
            }
            let path = std::path::PathBuf::from(path_text);
            let dx = match direction_args.first() {
                Some(value) => match value.parse::<f64>() {
                    Ok(value) if value.is_finite() => value,
                    _ => {
                        self.command_line.push_error("NUMBERHOUSES: DX must be a finite number.");
                        return Some(Task::none());
                    }
                },
                None => 1.0,
            };
            let dy = match direction_args.get(1) {
                Some(value) => match value.parse::<f64>() {
                    Ok(value) if value.is_finite() => value,
                    _ => {
                        self.command_line.push_error("NUMBERHOUSES: DY must be a finite number.");
                        return Some(Task::none());
                    }
                },
                None => 0.0,
            };
            if dx == 0.0 && dy == 0.0 {
                self.command_line.push_error("NUMBERHOUSES: direction must not be zero.");
                return Some(Task::none());
            }
            match crate::survey::parse_file(&path) {
                Ok(import) => {
                    if !self.tabs[i].scene.document.layers.contains("SO NHA") {
                        let _ = self.tabs[i].scene.document.layers.add(codec::Layer::new("SO NHA"));
                    }
                    let entities = crate::survey::house_number_text_entities(&import, crate::survey::HouseNumberingConfig {
                        start_x: 0.0, start_y: 0.0, dir_x: dx, dir_y: dy,
                    });
                    self.command_line.push_output(crate::tf!("NUMBERHOUSES: {} house label(s) ready.", entities.len()).as_ref());
                    return Some(self.apply_cmd_result(crate::command::CmdResult::CommitEntitiesAndExit(entities)))
                }
                Err(e) => {
                    self.command_line.push_error(crate::tf!("NUMBERHOUSES: {e}").as_ref());
                    return Some(Task::none())
                }
            }
        }

        if let Some(rest) = cmd.strip_prefix("EXPORTDXF").map(str::trim) {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                self.command_line.push_error("EXPORTDXF: specify survey TXT/CSV input and optional DXF output path.");
                return Some(Task::none());
            }
            let input = std::path::PathBuf::from(parts[0].trim_matches('"'));
            let output = parts.get(1).map(|v| std::path::PathBuf::from(v.trim_matches('"')))
                .unwrap_or_else(|| input.with_extension("dxf"));
            let dx = parts.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
            let dy = parts.get(3).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            if dx == 0.0 && dy == 0.0 {
                self.command_line.push_error("EXPORTDXF: direction must not be zero.");
                return Some(Task::none());
            }
            match crate::survey::parse_file(&input) {
                Ok(import) => match crate::survey::export_dxf_survey_file(&output, &import, crate::survey::HouseNumberingConfig {
                    start_x: 0.0, start_y: 0.0, dir_x: dx, dir_y: dy,
                }) {
                    Ok(()) => {
                        let count = import.drawable_points().count();
                        self.command_line.push_output(crate::tf!("EXPORTDXF: wrote {} survey point(s) plus line/text geometry to {}.", count, output.display()).as_ref());
                    }
                    Err(e) => self.command_line.push_error(crate::tf!("EXPORTDXF: {e}").as_ref()),
                },
                Err(e) => self.command_line.push_error(crate::tf!("EXPORTDXF: {e}").as_ref()),
            }
            return Some(Task::none());
        }

        if let Some(rest) = cmd.strip_prefix("EXPORTKML").map(str::trim) {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                self.command_line.push_error("EXPORTKML: specify survey TXT/CSV input and KML output path.");
                return Some(Task::none());
            }
            let input = std::path::PathBuf::from(parts[0].trim_matches('"'));
            let output = parts.get(1).map(|v| std::path::PathBuf::from(v.trim_matches('"')))
                .unwrap_or_else(|| input.with_extension("kml"));
            match crate::survey::parse_file(&input) {
                Ok(import) => {
                    let houses = crate::survey::house_features(&import);
                    let numbers = crate::survey::assign_house_numbers(&houses, crate::survey::HouseNumberingConfig {
                        start_x: 0.0, start_y: 0.0, dir_x: 1.0, dir_y: 0.0,
                    });
                    let mut house_by_point = std::collections::BTreeMap::new();
                    for n in numbers { if let Some(h)=houses.iter().find(|h| h.id==n.house_id) { for id in &h.point_ids { house_by_point.insert(*id,n.number); } } }
                    let records = import.points.iter().filter(|p| p.feature != crate::survey::FeatureKind::ProfileStation).map(|p| crate::survey::SurveyExportRecord {
                        x:p.x, y:p.y, z:p.z, code:p.code.clone(), house_number:house_by_point.get(&p.id).copied(),
                    }).collect::<Vec<_>>();
                    match crate::survey::export_kml_file(&output, &records) {
                        Ok(()) => { self.command_line.push_output(crate::tf!("EXPORTKML: wrote {} record(s) to {}.", records.len(), output.display()).as_ref()); }
                        Err(e) => { self.command_line.push_error(crate::tf!("EXPORTKML: {e}").as_ref()); }
                    }
                }
                Err(e) => self.command_line.push_error(crate::tf!("EXPORTKML: {e}").as_ref()),
            }
            return Some(Task::none());
        }

        let Some(rest) = cmd.strip_prefix("IMPORTSURVEY").map(str::trim) else {
            return None;
        };

        if rest.is_empty() {
            let command = crate::survey::SurveyImportCommand::new();
            self.command_line.push_info(&command.prompt());
            self.tabs[i].active_cmd = Some(Box::new(command));
            return Some(self.finish_dispatch(cmd));
        }

        let path = std::path::PathBuf::from(rest.trim_matches('"'));
        match crate::survey::parse_file(&path) {
            Ok(import) => {
                let unknown = import.unknown_codes.len();
                let profile = import.points.iter()
                    .filter(|p| p.feature == crate::survey::FeatureKind::ProfileStation)
                    .count();
                let entities = crate::survey::survey_entities(&import);
                let drawable = entities.len();
                if drawable == 0 {
                    self.command_line.push_info(
                        crate::tf!(
                            "IMPORTSURVEY: {} points imported; {} profile stations skipped; no drawable points.",
                            import.points.len(), profile
                        ).as_ref()
                    );
                    if unknown > 0 {
                        self.command_line.push_info(
                            crate::tf!("IMPORTSURVEY: {} unknown code(s) retained as UNKNOWN.", unknown).as_ref()
                        );
                    }
                    return Some(Task::none());
                }
                // Create missing SurveyCAD layers without replacing any
                // existing layer definition or user display settings.
                for name in [
                    "TRAC DIEM", "GIAO THONG", "THUY HE", "TUONG RAO", "NHA DAN",
                    "CONG TRINH", "DIEN", "DIA HINH", "CONG", "CAY", "MO", "RUONG",
                    "CANH QUAN", "UNKNOWN",
                ] {
                    if !self.tabs[i].scene.document.layers.contains(name) {
                        let _ = self.tabs[i].scene.document.layers.add(codec::Layer::new(name));
                    }
                }

                self.command_line.push_output(
                    crate::tf!(
                        "IMPORTSURVEY: {} native entity(s) ready; {} profile station(s) skipped; {} unknown code(s).",
                        drawable, profile, unknown
                    ).as_ref()
                );
                Some(self.apply_cmd_result(crate::command::CmdResult::CommitEntitiesAndExit(entities)))
            }
            Err(e) => {
                self.command_line.push_error(
                    crate::tf!("IMPORTSURVEY: {e}").as_ref()
                );
                Some(Task::none())
            }
        }
    }
}

inventory::submit!(crate::command::CommandRegistration {
    names: &["IMPORTSURVEY", "SURVEYIMPORT", "NUMBERHOUSES", "EXPORTKML", "EXPORTDXF"]
});
