use crate::file_analyser::FileAnalyser;
use crate::manifests::installer_manifest::{NestedInstallerFiles, NestedInstallerType};
use crate::types::architecture::Architecture;
use crate::url_utils::VALID_FILE_EXTENSIONS;
use color_eyre::eyre::Result;
use inquire::{min_length, MultiSelect};
use memmap2::Mmap;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::io::{Cursor, Read, Seek};
use std::path::Path;
use std::{io, mem};
use zip::ZipArchive;

pub struct Zip {
    pub nested_installer_type: Option<NestedInstallerType>,
    pub nested_installer_files: Option<BTreeSet<NestedInstallerFiles>>,
    pub architecture: Option<Architecture>,
    identified_files: Vec<String>,
}

impl Zip {
    pub fn new<R: Read + Seek>(reader: R, relative_file_path: Option<&str>) -> Result<Self> {
        let mut zip = ZipArchive::new(reader)?;

        let mut identified_files = zip
            .file_names()
            .filter(|file_name| {
                VALID_FILE_EXTENSIONS.iter().any(|file_extension| {
                    Path::new(file_name).extension().map_or(false, |extension| {
                        extension.eq_ignore_ascii_case(file_extension)
                    })
                })
            })
            .map(|path| (*path).to_string())
            .collect::<Vec<_>>();

        let installer_type_counts = VALID_FILE_EXTENSIONS
            .iter()
            .map(|file_extension| {
                (
                    file_extension,
                    zip.file_names()
                        .filter(|file_name| {
                            Path::new(file_name).extension().map_or(false, |extension| {
                                extension.eq_ignore_ascii_case(file_extension)
                            })
                        })
                        .count(),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut nested_installer_type = None;
        let mut architecture = None;

        // If there's only one valid file in the zip, extract and analyse it
        if installer_type_counts
            .values()
            .filter(|&&count| count == 1)
            .count()
            == 1
        {
            let chosen_file_name = (*identified_files.swap_remove(0)).to_string();
            if let Ok(mut chosen_file) = zip.by_name(&chosen_file_name) {
                let mut temp_file = tempfile::tempfile()?;
                io::copy(&mut chosen_file, &mut temp_file)?;
                let map = unsafe { Mmap::map(&temp_file) }?;
                let cursor = Cursor::new(map.as_ref());
                let file_analyser =
                    FileAnalyser::new(cursor, Cow::Borrowed(&chosen_file_name), true, None)?;
                nested_installer_type = file_analyser.installer_type.to_nested();
                architecture = file_analyser.architecture;
            }
            return Ok(Self {
                nested_installer_type,
                nested_installer_files: Some(BTreeSet::from([NestedInstallerFiles {
                    relative_file_path: chosen_file_name,
                    portable_command_alias: None,
                }])),
                architecture,
                identified_files: Vec::new(),
            });
        }

        if let Some(path) = relative_file_path {
            if let Ok(mut chosen_file) = zip.by_name(path) {
                let mut temp_file = tempfile::tempfile()?;
                io::copy(&mut chosen_file, &mut temp_file)?;
                let map = unsafe { Mmap::map(&temp_file) }?;
                let cursor = Cursor::new(map.as_ref());
                let file_analyser = FileAnalyser::new(cursor, Cow::Borrowed(path), true, None)?;
                nested_installer_type = file_analyser.installer_type.to_nested();
                architecture = file_analyser.architecture;
            }
            return Ok(Self {
                nested_installer_type,
                nested_installer_files: Some(BTreeSet::from([NestedInstallerFiles {
                    relative_file_path: path.to_string(),
                    portable_command_alias: None,
                }])),
                architecture,
                identified_files: Vec::new(),
            });
        }

        Ok(Self {
            nested_installer_type,
            nested_installer_files: None,
            architecture,
            identified_files,
        })
    }

    pub fn prompt(&mut self) -> Result<()> {
        if !&self.identified_files.is_empty() {
            let chosen = MultiSelect::new(
                "Select the nested files",
                mem::take(&mut self.identified_files),
            )
            .with_validator(min_length!(1))
            .prompt()?;
            self.nested_installer_files = Some(
                chosen
                    .into_iter()
                    .map(|path| NestedInstallerFiles {
                        relative_file_path: path,
                        portable_command_alias: None, // Prompt if portable
                    })
                    .collect(),
            );
        }
        Ok(())
    }
}
