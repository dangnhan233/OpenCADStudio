use super::*;

impl OpenCADStudio {
    pub(super) fn dispatch_survey(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {

        if let Some(args) = cmd.strip_prefix("NUMBERHOUSES").map(str::trim) {
            let parts: Vec<&str> = args.split_whitespace().collect();
            if parts.is_empty() {
                self.command_line.push_error("NUMBERHOUSES: specify survey TXT/CSV path and optional direction DX DY.");
                return Some(Task::none());
            }
            let path = std::path::PathBuf::from(parts[0].trim_matches('"'));
            let dx = parts.get(1).and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
            let dy = parts.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
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
                    Some(self.apply_cmd_result(crate::command::CmdResult::CommitEntitiesAndExit(entities)))
                }
                Err(e) => {
                    self.command_line.push_error(crate::tf!("NUMBERHOUSES: {e}").as_ref());
                    Some(Task::none())
                }
            }
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
    names: &["IMPORTSURVEY", "SURVEYIMPORT", "NUMBERHOUSES"]
});
