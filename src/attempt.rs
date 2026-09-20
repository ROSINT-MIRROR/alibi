use crate::error::Error;

pub struct Trail {
    steps: Vec<Step>,
}

struct Step {
    target: String,
    cause: Error,
}

impl Trail {
    pub fn new() -> Result<Trail, Error> {
        Ok(Trail { steps: Vec::new() })
    }

    pub fn record(&mut self, target: &str, cause: Error) -> Result<(), Error> {
        self.steps.push(Step {
            target: target.to_string(),
            cause,
        });
        Ok(())
    }

    pub fn into_error(self) -> Result<Error, Error> {
        let mut steps = self.steps;
        let Some(last) = steps.pop() else {
            return Ok(Error::NoMirror);
        };
        if steps.is_empty() {
            return Ok(last.cause);
        }
        Ok(Error::Network(render(&steps, &last)))
    }
}

fn render(earlier: &[Step], last: &Step) -> String {
    let mut out = String::new();
    for step in earlier {
        out.push_str(&format!("{}: {}; ", step.target, step.cause));
    }
    out.push_str(&format!("{}: {}", last.target, last.cause));
    out
}
