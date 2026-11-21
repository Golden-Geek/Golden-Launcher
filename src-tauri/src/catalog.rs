use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppId {
    Chataigne,
    Chataigne2,
    MapGyver,
    MapGyver2,
    Blux,
    BlinderKitten,
    BenTo,
    Lgml,
}

impl AppId {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Chataigne => "chataigne",
            Self::Chataigne2 => "chataigne2",
            Self::MapGyver => "map-gyver",
            Self::MapGyver2 => "map-gyver2",
            Self::Blux => "blux",
            Self::BlinderKitten => "blinder-kitten",
            Self::BenTo => "ben-to",
            Self::Lgml => "lgml",
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub fn executable(self) -> &'static str {
        match self {
            Self::Chataigne => "Chataigne",
            Self::Chataigne2 => "Chataigne2",
            Self::MapGyver => "MapGyver",
            Self::MapGyver2 => "MapGyver2",
            Self::Blux => "Blux",
            Self::BlinderKitten => "BlinderKitten",
            Self::BenTo => "BenTo",
            Self::Lgml => "LGML",
        }
    }
}

#[derive(Clone, Copy, Serialize)]
pub struct Product {
    pub id: AppId,
    pub name: &'static str,
    pub description: &'static str,
}

pub const PRODUCTS: [Product; 8] = [
    Product {
        id: AppId::Chataigne,
        name: "Chataigne",
        description: "Connect devices, software, and show-control workflows.",
    },
    Product {
        id: AppId::Chataigne2,
        name: "Chataigne 2",
        description: "The next generation of modular show control.",
    },
    Product {
        id: AppId::MapGyver,
        name: "MapGyver",
        description: "Compose video and map projections to physical spaces.",
    },
    Product {
        id: AppId::MapGyver2,
        name: "MapGyver 2",
        description: "The next generation of video composition and projection mapping.",
    },
    Product {
        id: AppId::Blux,
        name: "Blux",
        description: "Play with fixtures, effects, and live lighting.",
    },
    Product {
        id: AppId::BlinderKitten,
        name: "BlinderKitten",
        description: "Program and run live lighting shows.",
    },
    Product {
        id: AppId::BenTo,
        name: "BenTo",
        description: "Design and animate interactive LED installations.",
    },
    Product {
        id: AppId::Lgml,
        name: "Le Grand Mechant Loop",
        description: "Perform with audio loops, effects, and live routing.",
    },
];
