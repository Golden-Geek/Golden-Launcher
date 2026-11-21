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
