// Removed from understanding/src/summary.rs in #415: `ClaudeWriter::name_command`, which nothing called.

    /// `name_request` as the process path runs it with the program that writes now.
    pub fn name_command(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
    ) -> (Command, String) {
        self.build(&self.name_request(setup, prompt, response))
    }
