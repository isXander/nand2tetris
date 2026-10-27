import org.jetbrains.intellij.platform.gradle.tasks.PrepareSandboxTask
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinVersion

plugins {
    kotlin("jvm") version "2.2.20"
    id("org.jetbrains.intellij.platform") version "2.19.0"
}

group = "dev.isxander.nand2tetris"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform { defaultRepositories() }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity("2025.2.4")
        bundledPlugin("org.jetbrains.plugins.textmate")
        plugin("com.redhat.devtools.lsp4ij:0.21.0")
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_21
        apiVersion = KotlinVersion.KOTLIN_2_1
        languageVersion = KotlinVersion.KOTLIN_2_1
    }
}
java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

intellijPlatform {
    pluginConfiguration {
        ideaVersion { sinceBuild = "252" }
    }
    buildSearchableOptions = false

    // Credentials come from the environment (see .github/workflows/publish-idea.yml).
    signing {
        certificateChain = providers.environmentVariable("CERTIFICATE_CHAIN")
        privateKey = providers.environmentVariable("PRIVATE_KEY")
        password = providers.environmentVariable("PRIVATE_KEY_PASSWORD")
    }
    publishing {
        token = providers.environmentVariable("PUBLISH_TOKEN")
        // `1.2.3-beta.1` goes to the `beta` channel, everything else to the default one.
        channels = listOf(project.version.toString().substringAfter('-', "").substringBefore('.').ifEmpty { "default" })
    }
}

// TextMate needs a real directory on disk, rather than resources inside a JAR.
// Both the sandbox and the distributable get the canonical shared bundle.
tasks.withType<PrepareSandboxTask>().configureEach {
    from("../../grammars/textmate") {
        into("${project.name}/textmate/nand2tetris")
    }
}
