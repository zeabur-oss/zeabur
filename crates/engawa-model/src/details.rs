//! A project's details: the name, description and icon a person gives it, kept as annotations on
//! its namespace (`engawa.engawahq.dev/display-name`, `…/description`, `…/icon`). Annotations
//! only: there is no object inside the namespace to fall back to, so removing Engawa leaves
//! nothing but three annotations behind.

use std::collections::BTreeMap;

use engawa_core::names::{ANNOTATION_DESCRIPTION, ANNOTATION_DISPLAY_NAME, ANNOTATION_ICON};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The longest display name, in characters: a name, not a sentence.
pub const DISPLAY_NAME_MAX: usize = 64;

/// The longest description, in characters.
pub const DESCRIPTION_MAX: usize = 500;

/// The icons a project may have: a built-in set, so nothing is uploaded and the interface draws
/// each from its own icon font. An annotation naming another value is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectIcon {
    /// A folder (the default look of a project).
    Folder,
    /// A globe: a website.
    Globe,
    /// A shop front.
    Storefront,
    /// A rocket.
    Rocket,
    /// A cube: a component or package.
    Cube,
    /// A database.
    Database,
    /// A cloud.
    Cloud,
    /// Code.
    Code,
    /// A terminal: tools and scripts.
    Terminal,
    /// A chart: analytics.
    Chart,
    /// Speech bubbles: chat or messaging.
    Chat,
    /// A book: documentation.
    Book,
    /// A flask: experiments.
    Flask,
    /// A game controller.
    Game,
    /// A heart.
    Heart,
    /// A briefcase: business tools.
    Briefcase,
}

impl ProjectIcon {
    /// Every icon, in the order a picker shows them.
    pub const ALL: [Self; 16] = [
        Self::Folder,
        Self::Globe,
        Self::Storefront,
        Self::Rocket,
        Self::Cube,
        Self::Database,
        Self::Cloud,
        Self::Code,
        Self::Terminal,
        Self::Chart,
        Self::Chat,
        Self::Book,
        Self::Flask,
        Self::Game,
        Self::Heart,
        Self::Briefcase,
    ];

    /// The icon an annotation value names; `None` for any other value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|icon| icon.as_str() == value)
    }

    /// The annotation value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Globe => "globe",
            Self::Storefront => "storefront",
            Self::Rocket => "rocket",
            Self::Cube => "cube",
            Self::Database => "database",
            Self::Cloud => "cloud",
            Self::Code => "code",
            Self::Terminal => "terminal",
            Self::Chart => "chart",
            Self::Chat => "chat",
            Self::Book => "book",
            Self::Flask => "flask",
            Self::Game => "game",
            Self::Heart => "heart",
            Self::Briefcase => "briefcase",
        }
    }
}

/// A project's details, as its namespace's annotations give them. Each is absent when its
/// annotation is missing or blank (or, for the icon, names no built-in icon).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetails {
    /// The name to show instead of the namespace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// What the project is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Its icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<ProjectIcon>,
}

impl ProjectDetails {
    /// The details a namespace's annotations hold.
    #[must_use]
    pub fn from_annotations(annotations: Option<&BTreeMap<String, String>>) -> Self {
        let text = |key: &str| {
            annotations
                .and_then(|a| a.get(key))
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        Self {
            display_name: text(ANNOTATION_DISPLAY_NAME),
            description: text(ANNOTATION_DESCRIPTION),
            icon: text(ANNOTATION_ICON).and_then(|v| ProjectIcon::parse(&v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_reads_back_from_its_annotation_value() {
        for icon in ProjectIcon::ALL {
            assert_eq!(ProjectIcon::parse(icon.as_str()), Some(icon));
            // Serde's kebab-case name of a one-word variant is its lowercase name.
            assert_eq!(format!("{icon:?}").to_lowercase(), icon.as_str());
        }
        assert_eq!(ProjectIcon::parse("https://example.com/logo.png"), None);
    }

    #[test]
    fn blank_or_unknown_annotations_are_no_details() {
        let annotations = BTreeMap::from([
            (ANNOTATION_DISPLAY_NAME.to_owned(), "  Shop  ".to_owned()),
            (ANNOTATION_DESCRIPTION.to_owned(), "   ".to_owned()),
            (ANNOTATION_ICON.to_owned(), "unicorn".to_owned()),
        ]);
        assert_eq!(
            ProjectDetails::from_annotations(Some(&annotations)),
            ProjectDetails {
                display_name: Some("Shop".to_owned()),
                ..ProjectDetails::default()
            }
        );
        assert_eq!(
            ProjectDetails::from_annotations(None),
            ProjectDetails::default()
        );
    }
}
