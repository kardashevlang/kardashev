// kardfmt — the kardashev source formatter (backing `kard fmt`).
//
// Usage:
//   kardfmt <file.kd>             # print canonical formatting to stdout
//   kardfmt --check <file.kd>     # exit 0 if already formatted, 1 if not
//                                   (prints nothing on success; CI usage)
//   kardfmt -w <file.kd>          # rewrite the file in place
//
// The formatter parses the file to an AST (lexer + parser only — no LLVM,
// no typecheck) and pretty-prints canonical source via formatProgram(). It
// deliberately does NOT apply the kardc prelude: it formats exactly the
// declarations the user wrote, nothing injected.
//
// Idempotency: formatProgram is a pure function of the parse tree, and the
// canonical output re-parses to the same tree, so `fmt(fmt(src)) == fmt(src)`
// byte-for-byte. `--check` leverages this: a file is "formatted" iff its
// bytes equal formatProgram(parse(bytes)).

#include "kardashev/ast_print.hpp"
#include "kardashev/parser.hpp"

#include <cstdio>
#include <fstream>
#include <iostream>
#include <optional>
#include <sstream>
#include <string>

namespace {

std::optional<std::string> readFile(const std::string& path) {
    std::ifstream f(path);
    if (!f) return std::nullopt;
    std::ostringstream ss;
    ss << f.rdbuf();
    return ss.str();
}

void usage() {
    std::cerr << "usage: kardfmt <file.kd>            # print formatted source\n"
                 "       kardfmt --check <file.kd>     # exit non-zero if unformatted\n"
                 "       kardfmt -w <file.kd>          # rewrite file in place\n";
}

} // namespace

int main(int argc, char** argv) {
    bool check = false;
    bool write = false;
    std::string path;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a == "--check") {
            check = true;
        } else if (a == "-w" || a == "--write") {
            write = true;
        } else if (a == "-h" || a == "--help") {
            usage();
            return 0;
        } else if (!a.empty() && a[0] == '-') {
            std::cerr << "kardfmt: unknown option `" << a << "`\n";
            usage();
            return 2;
        } else if (path.empty()) {
            path = std::move(a);
        } else {
            std::cerr << "kardfmt: too many input files\n";
            return 2;
        }
    }
    if (path.empty()) {
        usage();
        return 2;
    }
    if (check && write) {
        std::cerr << "kardfmt: --check and -w are mutually exclusive\n";
        return 2;
    }

    auto src = readFile(path);
    if (!src) {
        std::cerr << "kardfmt: cannot open file: " << path << '\n';
        return 1;
    }

    auto pr = kardashev::parse(*src);
    if (!pr.ok()) {
        for (const auto& e : pr.errors) {
            std::cerr << "kardfmt: parse error " << e.line << ":" << e.column
                      << ": " << e.message << '\n';
        }
        return 1;
    }

    std::string formatted = kardashev::formatProgram(pr.program);

    if (check) {
        if (formatted == *src) return 0;
        std::cerr << "kardfmt: " << path << " is not formatted\n";
        return 1;
    }

    if (write) {
        if (formatted == *src) return 0; // no-op write avoids touching mtime
        std::ofstream out(path, std::ios::trunc);
        if (!out) {
            std::cerr << "kardfmt: cannot write file: " << path << '\n';
            return 1;
        }
        out << formatted;
        return 0;
    }

    std::cout << formatted;
    return 0;
}
