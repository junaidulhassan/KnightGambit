#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    RawExecutable,
    TarGz { member: &'static str },
}

#[derive(Debug, Clone, Copy)]
pub struct BinaryVariant {
    pub label: &'static str,
    pub required_cpu_flags: &'static [&'static str],
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub format: ArchiveFormat,
}

#[derive(Debug, Clone, Copy)]
pub struct ExtraDownload {
    pub url: &'static str,
    pub sha256: &'static str,
    pub destination: &'static str,
    pub extract_into: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub enum BuildSystem {
    Make {
        directory: &'static str,
        arguments: &'static [&'static str],
    },
    Meson {
        options: &'static [&'static str],
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Prerequisite {
    pub program: &'static str,
    pub package: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub enum InstallRecipe {
    Binary {
        variants: &'static [BinaryVariant],
    },
    Source {
        archive_url: &'static str,
        sha256: &'static str,
        extras: &'static [ExtraDownload],
        build: BuildSystem,
        output: &'static str,
        prerequisites: &'static [Prerequisite],
        extra_packages: &'static [&'static str],
    },
}

#[derive(Debug, Clone, Copy)]
pub struct NetworkDownload {
    pub label: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct StrengthProfile {
    pub limited_nodes: [u64; 3],
    pub limited_depth: [u32; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct EngineDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub authors: &'static str,
    pub license: &'static str,
    pub homepage: &'static str,
    pub description: &'static str,
    pub logo: Option<&'static [u8]>,
    pub accent: (&'static str, &'static str),
    pub executable_names: &'static [&'static str],
    pub recipe: InstallRecipe,
    pub weights_option: Option<&'static str>,
    pub networks: &'static [NetworkDownload],
    pub strength: StrengthProfile,
}

impl EngineDefinition {
    pub fn monogram(&self) -> String {
        self.name.chars().take(2).collect()
    }

    pub fn install_summary(&self) -> &'static str {
        match self.recipe {
            InstallRecipe::Binary { .. } => "Official Linux binary from the upstream GitHub release",
            InstallRecipe::Source { .. } => "Built locally from the official upstream source release",
        }
    }
}

const ALPHA_BETA_STRENGTH: StrengthProfile = StrengthProfile {
    limited_nodes: [600, 12_000, 250_000],
    limited_depth: [3, 7, 14],
};

pub const STOCKFISH: EngineDefinition = EngineDefinition {
    id: "stockfish",
    name: "Stockfish",
    version: "19",
    authors: "The Stockfish developers",
    license: "GPL-3.0-or-later",
    homepage: "https://stockfishchess.org",
    description: "The strongest open-source chess engine in the world. Combines alpha-beta search with an efficiently updatable neural network (NNUE). Supports calibrated playing strength through UCI_Elo and Skill Level.",
    logo: None,
    accent: ("#2e7d4f", "#e8f3ec"),
    executable_names: &["stockfish"],
    recipe: InstallRecipe::Binary {
        variants: &[BinaryVariant {
            label: "x86-64 universal",
            required_cpu_flags: &[],
            url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-linux-x86-64-universal.tar.gz",
            sha256: "9defc0d4e55d49c65a6d042f3e571a39fcea499ade6dbe741b53b8c65e03611f",
            size: 81_388_977,
            format: ArchiveFormat::TarGz {
                member: "stockfish/stockfish-linux-x86-64-universal",
            },
        }],
    },
    weights_option: None,
    networks: &[],
    strength: ALPHA_BETA_STRENGTH,
};

pub const BERSERK: EngineDefinition = EngineDefinition {
    id: "berserk",
    name: "Berserk",
    version: "14",
    authors: "Jay Honnold",
    license: "GPL-3.0-or-later",
    homepage: "https://github.com/jhonnold/berserk",
    description: "A top-rated NNUE engine written in C with an aggressive, sharp playing style. Upstream publishes Windows binaries only, so KnightGambit compiles the official release source with its official network.",
    logo: None,
    accent: ("#9b2226", "#fbeaea"),
    executable_names: &["berserk"],
    recipe: InstallRecipe::Source {
        archive_url: "https://github.com/jhonnold/berserk/archive/refs/tags/14.tar.gz",
        sha256: "3031835f691cf6f42e31c4f385563ac44649ca1dc44e0b86459de8ebef9e1b2d",
        extras: &[ExtraDownload {
            url: "https://github.com/jhonnold/berserk/releases/download/14/berserk-9b84c340af7e.nn",
            sha256: "9b84c340af7e45f6e07f0046235ccb327f4ae0840c8ee2c4b97b99121e5c5084",
            destination: "src/berserk-9b84c340af7e.nn",
            extract_into: None,
        }],
        build: BuildSystem::Make {
            directory: "src",
            arguments: &["build", "CC=gcc", "ARCH=native"],
        },
        output: "src/berserk",
        prerequisites: &[
            Prerequisite { program: "make", package: "make" },
            Prerequisite { program: "gcc", package: "gcc" },
        ],
        extra_packages: &[],
    },
    weights_option: None,
    networks: &[],
    strength: ALPHA_BETA_STRENGTH,
};

pub const ETHEREAL: EngineDefinition = EngineDefinition {
    id: "ethereal",
    name: "Ethereal",
    version: "14.00",
    authors: "Andrew Grant",
    license: "GPL-3.0-or-later",
    homepage: "https://github.com/AndyGrant/Ethereal",
    description: "A clean, instructive alpha-beta engine. The open-source release uses Ethereal's hand-crafted evaluation, which gives it a classical, positional style. Compiled locally from the official source release.",
    logo: Some(include_bytes!("../../assets/engines/ethereal.png")),
    accent: ("#3a4f8a", "#e9edf8"),
    executable_names: &["ethereal", "Ethereal"],
    recipe: InstallRecipe::Source {
        archive_url: "https://github.com/AndyGrant/Ethereal/archive/refs/tags/v14.00.tar.gz",
        sha256: "ca2c578810b0dc9d19543806df1a5482d334f53f960a3192042cb8722953a528",
        extras: &[],
        build: BuildSystem::Make {
            directory: "src",
            arguments: &["basic", "CC=gcc"],
        },
        output: "src/Ethereal",
        prerequisites: &[
            Prerequisite { program: "make", package: "make" },
            Prerequisite { program: "gcc", package: "gcc" },
        ],
        extra_packages: &[],
    },
    weights_option: None,
    networks: &[],
    strength: ALPHA_BETA_STRENGTH,
};

pub const KOIVISTO: EngineDefinition = EngineDefinition {
    id: "koivisto",
    name: "Koivisto",
    version: "9.0",
    authors: "Kim Kåhre, Finn Eggers",
    license: "GPL-3.0-or-later",
    homepage: "https://github.com/Luecx/Koivisto",
    description: "A strong NNUE engine written in C++ with a solid, universal style. Installed from the official PGO-optimized Linux release, choosing the fastest build your processor supports.",
    logo: Some(include_bytes!("../../assets/engines/koivisto.png")),
    accent: ("#3f7d20", "#edf6e6"),
    executable_names: &["koivisto", "Koivisto"],
    recipe: InstallRecipe::Binary {
        variants: &[
            BinaryVariant {
                label: "AVX2",
                required_cpu_flags: &["avx2", "bmi2"],
                url: "https://github.com/Luecx/Koivisto/releases/download/v9.0/Koivisto_9.0-linux-avx2-pgo",
                sha256: "1c28f937c0dae37e7f3834f8d0ce318d77da33bf2d392b23597c4b4b67e99b36",
                size: 17_880_248,
                format: ArchiveFormat::RawExecutable,
            },
            BinaryVariant {
                label: "SSE4.2",
                required_cpu_flags: &["sse4_2", "popcnt"],
                url: "https://github.com/Luecx/Koivisto/releases/download/v9.0/Koivisto_9.0-linux-sse42-pgo",
                sha256: "b05a37a7df79f59bec5f61b9359841dbee560e716b218ec99bdefe0d2498d3cb",
                size: 17_876_152,
                format: ArchiveFormat::RawExecutable,
            },
            BinaryVariant {
                label: "SSE2",
                required_cpu_flags: &[],
                url: "https://github.com/Luecx/Koivisto/releases/download/v9.0/Koivisto_9.0-linux-sse2-pgo",
                sha256: "840cace0b75b240d93a98e3658dd8e0ac4885c36275159f02cfb55bf613449fb",
                size: 17_876_152,
                format: ArchiveFormat::RawExecutable,
            },
        ],
    },
    weights_option: None,
    networks: &[],
    strength: ALPHA_BETA_STRENGTH,
};

pub const LC0: EngineDefinition = EngineDefinition {
    id: "lc0",
    name: "Leela Chess Zero",
    version: "0.32.1",
    authors: "The LCZero Authors",
    license: "GPL-3.0-or-later",
    homepage: "https://lczero.org",
    description: "A neural-network engine inspired by AlphaZero that searches with Monte Carlo tree search. Plays a strikingly human, strategic game. Requires a separate network weights file, which is optional and downloaded only on request.",
    logo: None,
    accent: ("#1f5f8b", "#e6f0f7"),
    executable_names: &["lc0"],
    recipe: InstallRecipe::Source {
        archive_url: "https://github.com/LeelaChessZero/lc0/archive/refs/tags/v0.32.1.tar.gz",
        sha256: "78b17e8e8d29da30492c86fdac69c519816f4a9d9c3b787dc41a6f9e8d20fde3",
        extras: &[ExtraDownload {
            url: "https://github.com/LeelaChessZero/lczero-common/archive/b326b154221a6eb91977bdccf11d6f89e8547875.tar.gz",
            sha256: "d12ec2257c0412a8962c0c439170ce98c6001ab6be6a6444a849d0115bfb330a",
            destination: "lczero-common.tar.gz",
            extract_into: Some("libs/lczero-common"),
        }],
        build: BuildSystem::Meson {
            options: &["--buildtype=release", "-Dgtest=false"],
        },
        output: "build/lc0",
        prerequisites: &[
            Prerequisite { program: "meson", package: "meson" },
            Prerequisite { program: "ninja", package: "ninja-build" },
            Prerequisite { program: "g++", package: "g++" },
            Prerequisite { program: "python3", package: "python3" },
        ],
        extra_packages: &["libopenblas-dev", "zlib1g-dev"],
    },
    weights_option: Some("WeightsFile"),
    networks: &[
        NetworkDownload {
            label: "T1-256x10 distilled (small, CPU friendly)",
            file_name: "t1-256x10-distilled-swa-2432500.pb.gz",
            url: "https://storage.lczero.org/files/networks-contrib/t1-256x10-distilled-swa-2432500.pb.gz",
            sha256: "bc27a6cae8ad36f2b9a80a6ad9dabb0d6fda25b1e7f481a79bc359e14f563406",
            size: 37_118_673,
        },
    ],
    strength: StrengthProfile {
        limited_nodes: [1, 24, 600],
        limited_depth: [0, 0, 0],
    },
};

pub const CATALOG: [EngineDefinition; 5] = [STOCKFISH, BERSERK, ETHEREAL, KOIVISTO, LC0];

pub fn find_definition(id: &str) -> Option<&'static EngineDefinition> {
    CATALOG.iter().find(|definition| definition.id == id)
}

pub const LC0_NETWORKS_PAGE: &str = "https://lczero.org/play/networks/bestnets/";
