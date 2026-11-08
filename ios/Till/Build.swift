/// The build's settings for the points of sale, as Android's Gradle
/// properties are (README). Placeholders until onboarding signs in (I2),
/// when they come from the build configuration. Both ids are public: the
/// till signs in with PKCE, so no secret is ever in the app.
enum Build {
    static let squareApplicationId = "sq0idp-REPLACE-ME"
    static let cloverApplicationId = "REPLACE-ME-CLOVER"
}
