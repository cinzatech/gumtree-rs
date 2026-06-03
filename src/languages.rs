//! Built-in [`LanguageProfile`] implementations for all supported tree-sitter
//! grammars and a file-extension based lookup.

use tree_sitter::Language;

use crate::language::LanguageProfile;

macro_rules! simple_profile {
    ($name:ident, $lang_expr:expr) => {
        pub struct $name;
        impl LanguageProfile for $name {
            fn language(&self) -> Language {
                $lang_expr.into()
            }
        }
    };
}

// --- Official tree-sitter grammars ---
simple_profile!(AgdaProfile, tree_sitter_agda::LANGUAGE);
simple_profile!(BashProfile, tree_sitter_bash::LANGUAGE);
simple_profile!(CProfile, tree_sitter_c::LANGUAGE);
simple_profile!(CppProfile, tree_sitter_cpp::LANGUAGE);
simple_profile!(CSharpProfile, tree_sitter_c_sharp::LANGUAGE);
simple_profile!(CssProfile, tree_sitter_css::LANGUAGE);
simple_profile!(EmbeddedTemplateProfile, tree_sitter_embedded_template::LANGUAGE);
simple_profile!(GoProfile, tree_sitter_go::LANGUAGE);
simple_profile!(HaskellProfile, tree_sitter_haskell::LANGUAGE);
simple_profile!(HtmlProfile, tree_sitter_html::LANGUAGE);
simple_profile!(JavaProfile, tree_sitter_java::LANGUAGE);
simple_profile!(JavaScriptProfile, tree_sitter_javascript::LANGUAGE);
simple_profile!(JsDocProfile, tree_sitter_jsdoc::LANGUAGE);
simple_profile!(JsonProfile, tree_sitter_json::LANGUAGE);
simple_profile!(JuliaProfile, tree_sitter_julia::LANGUAGE);
simple_profile!(OcamlProfile, tree_sitter_ocaml::LANGUAGE_OCAML);
simple_profile!(OcamlInterfaceProfile, tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE);
simple_profile!(PhpProfile, tree_sitter_php::LANGUAGE_PHP);
simple_profile!(PythonProfile, tree_sitter_python::LANGUAGE);
simple_profile!(RegexProfile, tree_sitter_regex::LANGUAGE);
simple_profile!(RubyProfile, tree_sitter_ruby::LANGUAGE);
simple_profile!(RustProfile, tree_sitter_rust::LANGUAGE);
simple_profile!(ScalaProfile, tree_sitter_scala::LANGUAGE);
simple_profile!(TypeScriptProfile, tree_sitter_typescript::LANGUAGE_TYPESCRIPT);
simple_profile!(TsxProfile, tree_sitter_typescript::LANGUAGE_TSX);
simple_profile!(VerilogProfile, tree_sitter_verilog::LANGUAGE);
simple_profile!(YamlProfile, tree_sitter_yaml::LANGUAGE);

// --- Community grammars ---
simple_profile!(CmakeProfile, tree_sitter_cmake::LANGUAGE);
simple_profile!(DartProfile, tree_sitter_dart::LANGUAGE);
simple_profile!(DockerfileProfile, tree_sitter_containerfile::LANGUAGE);
simple_profile!(ElixirProfile, tree_sitter_elixir::LANGUAGE);
simple_profile!(ErlangProfile, tree_sitter_erlang::LANGUAGE);
simple_profile!(KotlinProfile, tree_sitter_kotlin_ng::LANGUAGE);
simple_profile!(LatexProfile, tree_sitter_latex::LANGUAGE);
simple_profile!(LuaProfile, tree_sitter_lua::LANGUAGE);
simple_profile!(MakeProfile, tree_sitter_make::LANGUAGE);
simple_profile!(MarkdownProfile, tree_sitter_md::LANGUAGE);
simple_profile!(NixProfile, tree_sitter_nix::LANGUAGE);
simple_profile!(PerlProfile, tree_sitter_perl::LANGUAGE);
simple_profile!(ProtoProfile, tree_sitter_proto::LANGUAGE);
simple_profile!(RProfile, tree_sitter_r::LANGUAGE);
simple_profile!(SqlProfile, tree_sitter_sql::LANGUAGE);
simple_profile!(SwiftProfile, tree_sitter_swift::LANGUAGE);
simple_profile!(TomlProfile, tree_sitter_toml_ng::LANGUAGE);
simple_profile!(ZigProfile, tree_sitter_zig::LANGUAGE);

/// Return a `&dyn LanguageProfile` for a file extension (without the leading
/// dot), or `None` if the extension is not recognised.
pub fn profile_for_ext(ext: &str) -> Option<&'static dyn LanguageProfile> {
    static AGDA: AgdaProfile = AgdaProfile;
    static BASH: BashProfile = BashProfile;
    static C: CProfile = CProfile;
    static CMAKE: CmakeProfile = CmakeProfile;
    static CPP: CppProfile = CppProfile;
    static CSHARP: CSharpProfile = CSharpProfile;
    static CSS: CssProfile = CssProfile;
    static DART: DartProfile = DartProfile;
    static DOCKERFILE: DockerfileProfile = DockerfileProfile;
    static ELIXIR: ElixirProfile = ElixirProfile;
    static ERB: EmbeddedTemplateProfile = EmbeddedTemplateProfile;
    static ERLANG: ErlangProfile = ErlangProfile;
    static GO: GoProfile = GoProfile;
    static HASKELL: HaskellProfile = HaskellProfile;
    static HTML: HtmlProfile = HtmlProfile;
    static JAVA: JavaProfile = JavaProfile;
    static JS: JavaScriptProfile = JavaScriptProfile;
    static JSDOC: JsDocProfile = JsDocProfile;
    static JSON: JsonProfile = JsonProfile;
    static JULIA: JuliaProfile = JuliaProfile;
    static KOTLIN: KotlinProfile = KotlinProfile;
    static LATEX: LatexProfile = LatexProfile;
    static LUA: LuaProfile = LuaProfile;
    static MAKE: MakeProfile = MakeProfile;
    static MARKDOWN: MarkdownProfile = MarkdownProfile;
    static NIX: NixProfile = NixProfile;
    static OCAML: OcamlProfile = OcamlProfile;
    static OCAML_IFACE: OcamlInterfaceProfile = OcamlInterfaceProfile;
    static PERL: PerlProfile = PerlProfile;
    static PHP: PhpProfile = PhpProfile;
    static PROTO: ProtoProfile = ProtoProfile;
    static PYTHON: PythonProfile = PythonProfile;
    static R: RProfile = RProfile;
    static REGEX: RegexProfile = RegexProfile;
    static RUBY: RubyProfile = RubyProfile;
    static RUST: RustProfile = RustProfile;
    static SCALA: ScalaProfile = ScalaProfile;
    static SQL: SqlProfile = SqlProfile;
    static SWIFT: SwiftProfile = SwiftProfile;
    static TOML: TomlProfile = TomlProfile;
    static TS: TypeScriptProfile = TypeScriptProfile;
    static TSX: TsxProfile = TsxProfile;
    static VERILOG: VerilogProfile = VerilogProfile;
    static YAML: YamlProfile = YamlProfile;
    static ZIG: ZigProfile = ZigProfile;

    match ext {
        "agda" => Some(&AGDA),
        "sh" | "bash" | "zsh" => Some(&BASH),
        "c" | "h" => Some(&C),
        "cmake" => Some(&CMAKE),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" => Some(&CPP),
        "cs" => Some(&CSHARP),
        "css" => Some(&CSS),
        "dart" => Some(&DART),
        "dockerfile" | "Dockerfile" => Some(&DOCKERFILE),
        "ex" | "exs" => Some(&ELIXIR),
        "erb" | "ejs" => Some(&ERB),
        "erl" | "hrl" => Some(&ERLANG),
        "go" => Some(&GO),
        "hs" | "lhs" => Some(&HASKELL),
        "html" | "htm" => Some(&HTML),
        "java" => Some(&JAVA),
        "js" | "mjs" | "cjs" | "jsx" => Some(&JS),
        "jsdoc" => Some(&JSDOC),
        "json" => Some(&JSON),
        "jl" => Some(&JULIA),
        "kt" | "kts" => Some(&KOTLIN),
        "tex" | "latex" | "sty" | "cls" => Some(&LATEX),
        "lua" => Some(&LUA),
        "mk" | "makefile" | "Makefile" => Some(&MAKE),
        "md" | "markdown" => Some(&MARKDOWN),
        "nix" => Some(&NIX),
        "ml" => Some(&OCAML),
        "mli" => Some(&OCAML_IFACE),
        "pl" | "pm" => Some(&PERL),
        "php" => Some(&PHP),
        "proto" => Some(&PROTO),
        "py" | "pyi" => Some(&PYTHON),
        "r" | "R" => Some(&R),
        "regex" => Some(&REGEX),
        "rb" => Some(&RUBY),
        "rs" => Some(&RUST),
        "scala" | "sc" => Some(&SCALA),
        "sql" => Some(&SQL),
        "swift" => Some(&SWIFT),
        "toml" => Some(&TOML),
        "ts" | "mts" | "cts" => Some(&TS),
        "tsx" => Some(&TSX),
        "v" | "sv" | "svh" => Some(&VERILOG),
        "yaml" | "yml" => Some(&YAML),
        "zig" => Some(&ZIG),
        _ => None,
    }
}

/// List all supported file extensions.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        "agda", "bash", "c", "cc", "cmake", "cjs", "cls", "cpp", "cs",
        "css", "cts", "cxx", "dart", "dockerfile", "ejs", "erl", "erb",
        "ex", "exs", "go", "h", "hh", "hpp", "hrl", "hs", "htm", "html",
        "hxx", "java", "jl", "js", "jsdoc", "json", "jsx", "kt", "kts",
        "latex", "lhs", "lua", "makefile", "md", "mjs", "mk", "ml", "mli",
        "mts", "nix", "php", "pl", "pm", "proto", "py", "pyi", "r", "R",
        "rb", "regex", "rs", "sc", "scala", "sh", "sql", "sty", "sv",
        "svh", "swift", "tex", "toml", "ts", "tsx", "v", "yaml", "yml",
        "zig", "zsh",
    ]
}
