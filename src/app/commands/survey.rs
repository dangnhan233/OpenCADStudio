use super::*;

impl OpenCADStudio {
    pub(super) fn dispatch_survey(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {

        if let Some(args) = cmd.strip_prefix("NUMBERHOUSES").map(str::trim) {
            let values: Vec<f64> = args.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            let (dx,dy) = match values.as_slice() { [x,y,..] => (*x,*y), [] => (1.0,0.0), _ => (1.0,0.0) };
            if dx == 0.0 && dy == 0.0 {
                self.command_line.push_error("NUMBERHOUSES: direction must not be zero.");
                return Some(Task::none());
            }
            let import = self.tabs[i].scene.document.entities().filter_map(|_| None::<crate::survey::SurveyPoint>);
            let _ = import;
            self.command_line.push_error("NUMBERHOUSES: use IMPORTSURVEY <file> to create numbered houses in V1.4.");
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
    names: &["IMPORTSURVEY", "SURVEYIMPORT"]
});
