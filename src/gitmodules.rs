use std::collections::HashMap;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct GitSubmodule {
    name: String,
    path: PathBuf,
    url: String,
    branch: Option<String>,
}

#[derive(Debug)]
pub struct GitModules {
    submodules: Vec<GitSubmodule>,
}

impl GitModules {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let mut parser = GitModulesParser::new(input);
        let result = parser.parse()?;

        Ok(result)
    }

    pub fn contains(&self, path: &Path) -> bool {
        for submodule in &self.submodules {
            let _ = submodule.name;
            let _ = submodule.url;
            let _ = submodule.branch;

            if submodule.path == path {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, thiserror::Error)]
enum ParseError {
    #[error(transparent)]
    IO(#[from] IoError),
    #[error("invalid file: {0}")]
    InvalidFile(String),
    #[error("end of file")]
    EOF,
}

struct GitModulesParser<'a> {
    input: &'a str,
}

impl<'a> GitModulesParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input }
    }

    fn parse(&mut self) -> Result<GitModules, ParseError> {
        let mut result = GitModules {
            submodules: Vec::new(),
        };

        'l: while !self.input.is_empty() {
            match self.parse_submodule() {
                Ok(submodule) => result.submodules.push(submodule),
                Err(err) => match err {
                    ParseError::InvalidFile(_) => return Err(err),
                    ParseError::IO(err) => return Err(ParseError::from(err)),
                    ParseError::EOF => break 'l,
                },
            }
        }

        Ok(result)
    }

    fn parse_submodule(&mut self) -> Result<GitSubmodule, ParseError> {
        // Ignore any leading whitespace
        self.eat_whitespace();

        // Parse the [submodule "<foo>"] section
        let name = {
            self.expect_string("[submodule ")?;
            self.parse_quoted_string()?
        };

        // Parse all key values in the section

        let data = {
            let mut tmp = HashMap::new();

            'l: loop {
                self.eat_whitespace();

                if self.input.starts_with('[') {
                    break;
                }

                let key = match self.parse_until_or_err('=') {
                    Ok(key) => key,
                    Err(err) => match err {
                        ParseError::EOF => break 'l,
                        _ => return Err(err),
                    },
                };
                let value = self.parse_until_eol();

                tmp.insert(key, value);
            }

            tmp
        };

        Ok(GitSubmodule {
            name,
            path: PathBuf::from(data.get("path").map(|s| s.to_string()).unwrap_or_default()),
            url: data.get("url").map(|s| s.to_string()).unwrap_or_default(),
            branch: data.get("branch").map(|s| s.to_string()),
        })
    }

    fn parse_until_or_err(&mut self, ch: char) -> Result<String, ParseError> {
        match self.input.find(ch) {
            Some(pos) => {
                let result = self.input[..pos].trim().to_string();
                self.input = &self.input[pos + 1..];
                Ok(result)
            }
            None => Err(ParseError::EOF),
        }
    }

    fn parse_until(&mut self, ch: char) -> String {
        match self.input.find(ch) {
            Some(pos) => {
                let result = self.input[..pos].trim().to_string();
                self.input = &self.input[pos + 1..];
                result
            }
            None => {
                let result = self.input.trim().to_string();
                self.input = &self.input[self.input.len()..];
                result
            }
        }
    }

    fn parse_until_eol(&mut self) -> String {
        self.parse_until('\n')
    }

    fn parse_quoted_string(&mut self) -> Result<String, ParseError> {
        // Find the starting "
        if !self.input.starts_with('"') {
            return Err(ParseError::InvalidFile(
                "expected submodule name to start with a \"".to_string(),
            ));
        }
        self.input = &self.input[1..];

        // Find the ending "]
        match self.input.find("\"]") {
            Some(pos) => {
                let result = self.input[..pos].to_string();
                self.input = &self.input[pos + 2..];
                Ok(result)
            }
            None => Err(ParseError::InvalidFile(
                "expected submodule name to end with a \"".to_string(),
            )),
        }
    }

    fn expect_string(&mut self, expected: &str) -> Result<(), ParseError> {
        if !self.input.starts_with(expected) {
            return Err(ParseError::InvalidFile(format!(
                "string {} not found",
                expected
            )));
        }

        self.input = &self.input[expected.len()..];

        Ok(())
    }

    fn eat_whitespace(&mut self) {
        let mut pos = 0;

        for ch in self.input.bytes() {
            if ch == b' ' || ch == b'\t' || ch == b'\n' {
                pos += 1;
                continue;
            }

            break;
        }

        self.input = &self.input[pos..];
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_parse_gitmodules() {
        let contents = "
[submodule \"foobar\"]
    path = foo
    url = git@github.com:foo/bar.git
[submodule \"cpc\"]
    path = cpclol
    url = git@github.com:foo/cpc.git

[submodule \"yep\"]
    path = yop
    url = git@github.com:foo/yop.git
    branch = master
    foo = bar";

        let result = GitModules::parse(contents);
        assert!(result.is_ok());

        let gitmodules = result.unwrap();
        assert_eq!(3, gitmodules.submodules.len());

        let submodule1 = &gitmodules.submodules[0];
        assert_eq!("foobar", submodule1.name);
        assert_eq!("foo", submodule1.path.to_str().unwrap());
        assert_eq!("git@github.com:foo/bar.git", submodule1.url);

        let submodule2 = &gitmodules.submodules[1];
        assert_eq!("cpc", submodule2.name);
        assert_eq!("cpclol", submodule2.path.to_str().unwrap());
        assert_eq!("git@github.com:foo/cpc.git", submodule2.url);

        let submodule3 = &gitmodules.submodules[2];
        assert_eq!("yep", submodule3.name);
        assert_eq!("yop", submodule3.path.to_str().unwrap());
        assert_eq!("git@github.com:foo/yop.git", submodule3.url);
        assert_eq!(Some("master"), submodule3.branch.as_deref());
    }
}
