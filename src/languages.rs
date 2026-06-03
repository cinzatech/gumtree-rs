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
simple_profile!(CommonLispProfile, tree_sitter_commonlisp::LANGUAGE_COMMONLISP);
simple_profile!(DartProfile, tree_sitter_dart::LANGUAGE);
simple_profile!(DockerfileProfile, tree_sitter_containerfile::LANGUAGE);
simple_profile!(ElixirProfile, tree_sitter_elixir::LANGUAGE);
simple_profile!(ElmProfile, tree_sitter_elm::LANGUAGE);
simple_profile!(ErlangProfile, tree_sitter_erlang::LANGUAGE);
simple_profile!(FortranProfile, tree_sitter_fortran::LANGUAGE);
simple_profile!(GdScriptProfile, tree_sitter_gdscript::LANGUAGE);
simple_profile!(GlslProfile, tree_sitter_glsl::LANGUAGE_GLSL);
simple_profile!(GraphqlProfile, tree_sitter_graphql::LANGUAGE);
simple_profile!(GroovyProfile, tree_sitter_groovy::LANGUAGE);
simple_profile!(HclProfile, tree_sitter_hcl::LANGUAGE);
simple_profile!(KotlinProfile, tree_sitter_kotlin_ng::LANGUAGE);
simple_profile!(LatexProfile, tree_sitter_latex::LANGUAGE);
simple_profile!(LuaProfile, tree_sitter_lua::LANGUAGE);
simple_profile!(MakeProfile, tree_sitter_make::LANGUAGE);
simple_profile!(MarkdownProfile, tree_sitter_md::LANGUAGE);
simple_profile!(NginxProfile, tree_sitter_nginx::LANGUAGE);
simple_profile!(NixProfile, tree_sitter_nix::LANGUAGE);
simple_profile!(ObjectiveCProfile, tree_sitter_objc::LANGUAGE);
simple_profile!(PascalProfile, tree_sitter_pascal::LANGUAGE);
simple_profile!(PerlProfile, tree_sitter_perl::LANGUAGE);
simple_profile!(PowerShellProfile, tree_sitter_powershell::LANGUAGE);
simple_profile!(PrologProfile, tree_sitter_prolog::LANGUAGE);
simple_profile!(ProtoProfile, tree_sitter_proto::LANGUAGE);
simple_profile!(RProfile, tree_sitter_r::LANGUAGE);
simple_profile!(RacketProfile, tree_sitter_racket::LANGUAGE);
simple_profile!(SchemeProfile, tree_sitter_scheme::LANGUAGE);
simple_profile!(SolidityProfile, tree_sitter_solidity::LANGUAGE);
simple_profile!(SqlProfile, tree_sitter_sql::LANGUAGE);
simple_profile!(SwiftProfile, tree_sitter_swift::LANGUAGE);
simple_profile!(TomlProfile, tree_sitter_toml_ng::LANGUAGE);
simple_profile!(XmlProfile, tree_sitter_xml::LANGUAGE_XML);
simple_profile!(DtdProfile, tree_sitter_xml::LANGUAGE_DTD);
simple_profile!(ZigProfile, tree_sitter_zig::LANGUAGE);

/// Return a `&dyn LanguageProfile` for a file extension (without the leading
/// dot), or `None` if the extension is not recognised.
pub fn profile_for_ext(ext: &str) -> Option<&'static dyn LanguageProfile> {
    static AGDA: AgdaProfile = AgdaProfile;
    static BASH: BashProfile = BashProfile;
    static C: CProfile = CProfile;
    static CMAKE: CmakeProfile = CmakeProfile;
    static COMMONLISP: CommonLispProfile = CommonLispProfile;
    static CPP: CppProfile = CppProfile;
    static CSHARP: CSharpProfile = CSharpProfile;
    static CSS: CssProfile = CssProfile;
    static DART: DartProfile = DartProfile;
    static DOCKERFILE: DockerfileProfile = DockerfileProfile;
    static DTD: DtdProfile = DtdProfile;
    static ELIXIR: ElixirProfile = ElixirProfile;
    static ELM: ElmProfile = ElmProfile;
    static ERB: EmbeddedTemplateProfile = EmbeddedTemplateProfile;
    static ERLANG: ErlangProfile = ErlangProfile;
    static FORTRAN: FortranProfile = FortranProfile;
    static GDSCRIPT: GdScriptProfile = GdScriptProfile;
    static GLSL: GlslProfile = GlslProfile;
    static GO: GoProfile = GoProfile;
    static GRAPHQL: GraphqlProfile = GraphqlProfile;
    static GROOVY: GroovyProfile = GroovyProfile;
    static HASKELL: HaskellProfile = HaskellProfile;
    static HCL: HclProfile = HclProfile;
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
    static NGINX: NginxProfile = NginxProfile;
    static NIX: NixProfile = NixProfile;
    static OBJC: ObjectiveCProfile = ObjectiveCProfile;
    static OCAML: OcamlProfile = OcamlProfile;
    static OCAML_IFACE: OcamlInterfaceProfile = OcamlInterfaceProfile;
    static PASCAL: PascalProfile = PascalProfile;
    static PERL: PerlProfile = PerlProfile;
    static PHP: PhpProfile = PhpProfile;
    static POWERSHELL: PowerShellProfile = PowerShellProfile;
    static PROLOG: PrologProfile = PrologProfile;
    static PROTO: ProtoProfile = ProtoProfile;
    static PYTHON: PythonProfile = PythonProfile;
    static R: RProfile = RProfile;
    static RACKET: RacketProfile = RacketProfile;
    static REGEX: RegexProfile = RegexProfile;
    static RUBY: RubyProfile = RubyProfile;
    static RUST: RustProfile = RustProfile;
    static SCALA: ScalaProfile = ScalaProfile;
    static SCHEME: SchemeProfile = SchemeProfile;
    static SOLIDITY: SolidityProfile = SolidityProfile;
    static SQL: SqlProfile = SqlProfile;
    static SWIFT: SwiftProfile = SwiftProfile;
    static TOML: TomlProfile = TomlProfile;
    static TS: TypeScriptProfile = TypeScriptProfile;
    static TSX: TsxProfile = TsxProfile;
    static VERILOG: VerilogProfile = VerilogProfile;
    static XML: XmlProfile = XmlProfile;
    static YAML: YamlProfile = YamlProfile;
    static ZIG: ZigProfile = ZigProfile;

    match ext {
        "agda" => Some(&AGDA),
        "sh" | "bash" | "zsh" => Some(&BASH),
        "c" | "h" => Some(&C),
        "cl" | "lisp" | "lsp" | "asd" => Some(&COMMONLISP),
        "cmake" => Some(&CMAKE),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" => Some(&CPP),
        "cs" => Some(&CSHARP),
        "css" => Some(&CSS),
        "dart" => Some(&DART),
        "dockerfile" | "Dockerfile" => Some(&DOCKERFILE),
        "dtd" => Some(&DTD),
        "ex" | "exs" => Some(&ELIXIR),
        "elm" => Some(&ELM),
        "erb" | "ejs" => Some(&ERB),
        "erl" | "hrl" => Some(&ERLANG),
        "f" | "f90" | "f95" | "f03" | "f08" | "for" | "fpp" => Some(&FORTRAN),
        "gd" => Some(&GDSCRIPT),
        "glsl" | "vert" | "frag" | "geom" | "comp" => Some(&GLSL),
        "go" => Some(&GO),
        "graphql" | "gql" => Some(&GRAPHQL),
        "groovy" | "gradle" => Some(&GROOVY),
        "hs" | "lhs" => Some(&HASKELL),
        "hcl" | "tf" | "tfvars" => Some(&HCL),
        "html" | "htm" => Some(&HTML),
        "java" => Some(&JAVA),
        "js" | "mjs" | "cjs" | "jsx" => Some(&JS),
        "jsdoc" => Some(&JSDOC),
        "json" => Some(&JSON),
        "jl" => Some(&JULIA),
        "kt" | "kts" => Some(&KOTLIN),
        "tex" | "latex" | "sty" | "cls" => Some(&LATEX),
        "lua" => Some(&LUA),
        "m" => Some(&OBJC),
        "mk" | "makefile" | "Makefile" => Some(&MAKE),
        "md" | "markdown" => Some(&MARKDOWN),
        "nginx" | "conf" => Some(&NGINX),
        "nix" => Some(&NIX),
        "ml" => Some(&OCAML),
        "mli" => Some(&OCAML_IFACE),
        "pas" | "pp" | "lpr" | "dpr" => Some(&PASCAL),
        "pl" | "pm" => Some(&PERL),
        "php" => Some(&PHP),
        "ps1" | "psm1" | "psd1" => Some(&POWERSHELL),
        "pro" | "P" | "prolog" => Some(&PROLOG),
        "proto" => Some(&PROTO),
        "py" | "pyi" => Some(&PYTHON),
        "r" | "R" => Some(&R),
        "rkt" => Some(&RACKET),
        "regex" => Some(&REGEX),
        "rb" => Some(&RUBY),
        "rs" => Some(&RUST),
        "scala" | "sc" => Some(&SCALA),
        "scm" | "ss" => Some(&SCHEME),
        "sol" => Some(&SOLIDITY),
        "sql" => Some(&SQL),
        "swift" => Some(&SWIFT),
        "toml" => Some(&TOML),
        "ts" | "mts" | "cts" => Some(&TS),
        "tsx" => Some(&TSX),
        "v" | "sv" | "svh" => Some(&VERILOG),
        "xml" | "xsl" | "xslt" | "xsd" | "svg" | "plist" => Some(&XML),
        "yaml" | "yml" => Some(&YAML),
        "zig" => Some(&ZIG),
        _ => None,
    }
}

/// List all supported file extensions.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        "agda", "asd", "bash", "c", "cc", "cjs", "cl", "cls", "cmake",
        "comp", "conf", "cpp", "cs", "css", "cts", "cxx", "dart",
        "dockerfile", "dpr", "dtd", "ejs", "elm", "erb", "erl", "ex",
        "exs", "f", "f03", "f08", "f90", "f95", "for", "fpp", "frag",
        "gd", "geom", "glsl", "go", "gql", "gradle", "graphql", "groovy",
        "h", "hcl", "hh", "hpp", "hrl", "hs", "htm", "html", "hxx",
        "java", "jl", "js", "jsdoc", "json", "jsx", "kt", "kts", "latex",
        "lhs", "lisp", "lpr", "lsp", "lua", "m", "makefile", "md",
        "markdown", "mjs", "mk", "ml", "mli", "mts", "nginx", "nix",
        "P", "pas", "php", "pl", "plist", "pm", "pp", "pro", "prolog",
        "proto", "ps1", "psd1", "psm1", "py", "pyi", "r", "R", "rb",
        "regex", "rkt", "rs", "sc", "scala", "scm", "sh", "sol", "sql",
        "ss", "sty", "sv", "svg", "svh", "swift", "tex", "tf", "tfvars",
        "toml", "ts", "tsx", "v", "vert", "xml", "xsd", "xsl", "xslt",
        "yaml", "yml", "zig", "zsh",
    ]
}
