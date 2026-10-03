import java.net.URI
import java.security.MessageDigest
import java.util.zip.ZipFile

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

abstract class PrepareFireContentSdk : DefaultTask() {
    @get:Input abstract val archiveUrl: Property<String>
    @get:Input abstract val archiveSha256: Property<String>
    @get:OutputFile abstract val sdkJar: RegularFileProperty

    @TaskAction
    fun prepare() {
        val jar = sdkJar.get().asFile
        jar.parentFile.mkdirs()
        val archive = jar.resolveSibling("content-sdk.zip")
        val connection = URI(archiveUrl.get()).toURL().openConnection().apply {
            connectTimeout = 30_000
            readTimeout = 30_000
        }
        connection.getInputStream().use { source -> archive.outputStream().use(source::copyTo) }
        val hash = MessageDigest.getInstance("SHA-256").digest(archive.readBytes())
            .joinToString("") { "%02x".format(it) }
        check(hash == archiveSha256.get()) { "Fire TV SDK archive checksum mismatch" }
        ZipFile(archive).use { zip ->
            val entry = zip.getEntry("com.amazon.tv.developer.sdk.content/com.amazon.tv.developer.content.sdk.jar")
                ?: error("Fire TV SDK archive is missing its compile-time JAR")
            zip.getInputStream(entry).use { source -> jar.outputStream().use(source::copyTo) }
        }
    }
}

val prepareFireContentSdk = tasks.register<PrepareFireContentSdk>("prepareFireContentSdk") {
    archiveUrl.set("https://amzndevresources.com/fire-tv/com.amazon.tv.developer.sdk.content.zip")
    archiveSha256.set("f3094973bbb18b5a58807ad043d31055ade6a2f6646bcae54a4d1022b9cdf593")
    sdkJar.set(layout.buildDirectory.file("fire-content-sdk/com.amazon.tv.developer.content.sdk.jar"))
}

val fireWatchActivityEnabled = providers.gradleProperty("duskcueFireWatchActivityEnabled")
    .orElse("false")
    .map { value -> value.toBooleanStrictOrNull() ?: error("duskcueFireWatchActivityEnabled must be true or false") }

val duskcueVersionCode = providers.gradleProperty("duskcueVersionCode")
    .orElse("1")
    .map { value ->
        value.toIntOrNull()?.takeIf { it in 1..2_100_000_000 }
            ?: error("duskcueVersionCode must be a positive Appstore-valid integer")
    }
val duskcueVersionName = providers.gradleProperty("duskcueVersionName")
    .orElse("0.1.0")
    .map { value -> value.trim().takeIf { it.isNotEmpty() } ?: error("duskcueVersionName must not be blank") }

android {
    namespace = "com.duskcue.tv"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.duskcue.firetv"
        minSdk = 28
        targetSdk = 36
        versionCode = duskcueVersionCode.get()
        versionName = duskcueVersionName.get()
        buildConfigField("boolean", "FIRE_WATCH_ACTIVITY_ENABLED", fireWatchActivityEnabled.get().toString())

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }

    sourceSets {
        getByName("main") {
            java.srcDirs(
                "../../android/app/src/main/java/com/duskcue/tv/api",
                "../../android/app/src/main/java/com/duskcue/tv/diagnostics",
                "../../android/app/src/main/java/com/duskcue/tv/home",
                "../../android/app/src/main/java/com/duskcue/tv/playback",
                "../../android/app/src/main/java/com/duskcue/tv/profiles",
                "../../android/app/src/main/java/com/duskcue/tv/session",
                "../../android/app/src/main/java/com/duskcue/tv/ui",
            )
            res.srcDir("../../android/app/src/main/res")
        }
        getByName("test") {
            java.srcDir("../../android/app/src/test/java/com/duskcue/tv/playback")
            java.srcDir("../../android/app/src/test/java/com/duskcue/tv/session")
            resources.srcDir("../../../../docs/api/fixtures/fire/v1")
            resources.srcDir("../../../../docs/api/fixtures/tv/v1")
            resources.srcDir("../../../../docs/api/fixtures/playback/v1")
            resources.srcDir("../../../../docs/api/fixtures/auth/v1")
            resources.srcDir("../../../../docs/api/fixtures/accessibility/v1")
            resources.srcDir("../../../../docs/api/fixtures/diagnostics/v1")
        }
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.06.00")
    compileOnly(files(prepareFireContentSdk.flatMap { it.sdkJar }).builtBy(prepareFireContentSdk))

    implementation(composeBom)
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.tv:tv-material:1.0.0")
    implementation("androidx.datastore:datastore-preferences:1.2.1")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("androidx.media3:media3-exoplayer:1.10.1")
    implementation("androidx.media3:media3-session:1.10.1")
    implementation("androidx.media3:media3-ui:1.10.1")

    debugImplementation("androidx.compose.ui:ui-tooling")
    testImplementation("junit:junit:4.13.2")
}
