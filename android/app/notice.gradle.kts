// The app's open source notice, generated with every build into the assets
// the Licenses page reads (`notice/` in the APK):
//
// - rust.json: every crate linked into the Rust core (cargo-about over
//   crates/pluto-ffi, offline, licenses from the crates' own files, policy
//   in crates/pluto-ffi/about.toml), the direct dependencies of pluto-ffi
//   and pluto-core, and the full notice text;
// - android.json: every library of the app's runtime classpath, with its
//   license and any NOTICE file it carries. A library from a group that is
//   not known to be Apache-2.0 stops the build until its license is
//   reviewed here;
// - texts/: Pluto's LICENSE, the Geist fonts' OFL and Lucide's ISC license.
//
// Applied by build.gradle.kts; run in `nix develop .#mobile` (cargo-about).

import groovy.json.JsonOutput
import groovy.json.JsonSlurper
import java.util.zip.ZipFile

val noticeRoot = layout.buildDirectory.dir("generated/notice")
// The assets directory build.gradle.kts adds: `notice/` in the APK.
val noticeDir = noticeRoot.map { it.dir("assets/notice") }
val repo = rootDir.parentFile
val core = File(repo, "crates/pluto-ffi")

// The libraries' groups and the license each ships under. JNA is offered
// under Apache-2.0 or LGPL-2.1; Pluto takes it under Apache-2.0.
val androidLicenses = linkedMapOf(
    "androidx." to "Apache-2.0",
    "org.jetbrains.kotlinx" to "Apache-2.0",
    "org.jetbrains.kotlin" to "Apache-2.0",
    "org.jetbrains" to "Apache-2.0",
    "com.google.guava" to "Apache-2.0",
    "com.google.errorprone" to "Apache-2.0",
    "org.jspecify" to "Apache-2.0",
    "net.java.dev.jna" to "Apache-2.0",
)

val rustAbout = tasks.register<Exec>("rustAbout") {
    description = "Lists the Rust core's crates and their licenses with cargo-about."
    workingDir = core
    val out = noticeRoot.map { it.file("about.json") }
    inputs.files(File(repo, "Cargo.lock"), File(core, "Cargo.toml"), File(core, "about.toml"), File(repo, "crates/pluto-core/Cargo.toml"))
    outputs.file(out)
    commandLine("cargo", "about", "generate", "--offline", "--format", "json", "-o", out.get().asFile.path)
}

val rustMetadata = tasks.register<Exec>("rustMetadata") {
    description = "Reads the direct dependencies of the Rust core."
    workingDir = repo
    val out = noticeRoot.map { it.file("metadata.json") }
    inputs.files(File(repo, "Cargo.lock"), File(core, "Cargo.toml"), File(repo, "crates/pluto-core/Cargo.toml"))
    outputs.file(out)
    commandLine("cargo", "metadata", "--offline", "--no-deps", "--format-version", "1")
    doFirst { standardOutput = out.get().asFile.outputStream() }
}

val rustNotice = tasks.register("rustNotice") {
    description = "Writes the Rust core's notice for the Licenses page."
    dependsOn(rustAbout, rustMetadata)
    val about = noticeRoot.map { it.file("about.json") }
    val metadata = noticeRoot.map { it.file("metadata.json") }
    val out = noticeDir.map { it.file("rust.json") }
    inputs.files(about, metadata)
    outputs.file(out)
    doLast {
        @Suppress("UNCHECKED_CAST")
        val report = JsonSlurper().parse(about.get().asFile) as Map<String, Any?>
        @Suppress("UNCHECKED_CAST")
        val packages = (JsonSlurper().parse(metadata.get().asFile) as Map<String, Any?>)["packages"] as List<Map<String, Any?>>
        val own = setOf("pluto-ffi", "pluto-core")
        // The normal dependencies of the core's two crates.
        val direct = sortedSetOf<String>()
        for (pkg in packages.filter { it["name"] in own }) {
            @Suppress("UNCHECKED_CAST")
            for (dep in pkg["dependencies"] as List<Map<String, Any?>>) {
                if (dep["kind"] == null && dep["name"] !in own) direct += dep["name"] as String
            }
        }
        val expressions = sortedMapOf<String, String>()
        val versions = sortedMapOf<String, MutableSet<String>>()
        @Suppress("UNCHECKED_CAST")
        for (entry in report["crates"] as List<Map<String, Any?>>) {
            @Suppress("UNCHECKED_CAST")
            val pkg = entry["package"] as Map<String, Any?>
            val name = pkg["name"] as String
            if (name in own) continue
            expressions.putIfAbsent(name, entry["license"] as? String ?: "")
            versions.getOrPut(name) { sortedSetOf() } += pkg["version"] as String
        }
        val texts = sortedMapOf<String, MutableList<Map<String, String>>>()
        val notice = StringBuilder()
        val mpl = sortedSetOf<String>()
        @Suppress("UNCHECKED_CAST")
        val licenses = report["licenses"] as List<Map<String, Any?>>
        for (license in licenses) {
            val id = license["id"] as String
            val name = license["name"] as String
            val text = (license["text"] as String).trim()
            val users = sortedSetOf<String>()
            @Suppress("UNCHECKED_CAST")
            for (user in license["used_by"] as List<Map<String, Any?>>) {
                @Suppress("UNCHECKED_CAST")
                val crate = user["crate"] as Map<String, Any?>
                val crateName = crate["name"] as String
                if (crateName in own) continue
                users += "$crateName ${crate["version"]}"
                val list = texts.getOrPut(crateName) { mutableListOf() }
                if (list.none { it["id"] == id && it["text"] == text }) list += mapOf("id" to id, "name" to name, "text" to text)
            }
            if (users.isEmpty()) continue
            if (id.contains("MPL")) mpl += users
            notice.append("$name ($id)\nUsed by: ${users.joinToString(", ")}\n\n$text\n\n${"-".repeat(72)}\n\n")
        }
        // MPL-2.0 asks to say where the source of what is shipped can be
        // obtained: crates.io keeps every published version's source.
        val sources = "The source code of every crate listed here is published on crates.io, " +
            "at https://crates.io/crates/<name>/<version>" +
            (if (mpl.isEmpty()) "." else ", including the MPL-2.0 ones: ${mpl.joinToString(", ")}.")
        notice.insert(0, "$sources\n\n${"-".repeat(72)}\n\n")
        val missing = direct.filter { it !in expressions }
        check(missing.isEmpty()) { "cargo-about did not list $missing" }
        // The canonical Apache-2.0 text: the one most crates carry. The
        // Android libraries' notice uses it too.
        val apache = licenses.filter { it["id"] == "Apache-2.0" }
            .maxByOrNull { (it["used_by"] as List<*>).size }?.get("text") as String? ?: ""
        val data = mapOf(
            "count" to expressions.size,
            "crates" to direct.map { name ->
                mapOf(
                    "name" to name,
                    "versions" to versions[name].orEmpty().toList(),
                    "license" to expressions[name],
                    "texts" to texts[name].orEmpty(),
                )
            },
            "notice" to notice.toString(),
            "sources" to sources,
            "apache" to apache.trim(),
        )
        out.get().asFile.apply { parentFile.mkdirs() }.writeText(JsonOutput.toJson(data))
    }
}

// The notices a library's jar carries (`META-INF/NOTICE*`, `NOTICE*`).
fun jarNotices(jar: File): List<String> = runCatching {
    ZipFile(jar).use { zip ->
        zip.entries().toList()
            .filter { !it.isDirectory && Regex("(META-INF/)?NOTICE(\\.txt|\\.md)?", RegexOption.IGNORE_CASE).matches(it.name) }
            .map { zip.getInputStream(it).bufferedReader().readText().trim() }
            .filter { it.isNotEmpty() }
    }
}.getOrDefault(emptyList())

// The notices of an artifact: a jar's, or the jars an AAR packs.
fun notices(file: File): List<String> {
    if (file.extension == "jar") return jarNotices(file)
    if (file.extension != "aar") return emptyList()
    return runCatching {
        ZipFile(file).use { aar ->
            aar.entries().toList().filter { it.name.endsWith(".jar") }.flatMap { entry ->
                val jar = File.createTempFile("notice", ".jar")
                try {
                    aar.getInputStream(entry).use { input -> jar.outputStream().use { input.copyTo(it) } }
                    jarNotices(jar)
                } finally {
                    jar.delete()
                }
            }
        }
    }.getOrDefault(emptyList())
}

val androidNotice = tasks.register("androidNotice") {
    description = "Writes the Android libraries' notice for the Licenses page."
    val out = noticeDir.map { it.file("android.json") }
    outputs.file(out)
    outputs.upToDateWhen { false }
    doLast {
        // Both build types: the debug APK also carries the Compose tooling.
        val libraries = sortedMapOf<String, MutableMap<String, Any>>()
        for (name in listOf("debugRuntimeClasspath", "releaseRuntimeClasspath")) {
            val configuration = project.configurations.getByName(name)
            val files = configuration.resolvedConfiguration.lenientConfiguration.artifacts
            for (artifact in files) {
                val id = artifact.moduleVersion.id
                val key = "${id.group}:${id.name}"
                val license = androidLicenses.entries.firstOrNull { (group, _) ->
                    id.group == group || id.group.startsWith(if (group.endsWith(".")) group else "$group.")
                }?.value ?: throw GradleException(
                    "$key has no reviewed license: check it and add its group to android/app/notice.gradle.kts",
                )
                val entry = libraries.getOrPut(key) {
                    mutableMapOf("group" to id.group, "name" to id.name, "version" to id.version, "license" to license, "notices" to mutableListOf<String>())
                }
                @Suppress("UNCHECKED_CAST")
                val list = entry["notices"] as MutableList<String>
                for (text in notices(artifact.file)) if (text !in list) list += text
            }
        }
        out.get().asFile.apply { parentFile.mkdirs() }.writeText(JsonOutput.toJson(libraries.values.toList()))
    }
}

val noticeFiles = tasks.register<Copy>("noticeFiles") {
    description = "Copies Pluto's license and the fonts' and icons' licenses."
    from(File(repo, "LICENSE"))
    from(File(projectDir, "licenses/OFL.txt"))
    from(File(core, "assets/LUCIDE-LICENSE"))
    into(noticeRoot.map { it.dir("assets/notice/texts") })
}

tasks.named("preBuild") { dependsOn(rustNotice, androidNotice, noticeFiles) }
