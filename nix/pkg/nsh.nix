{
  stdenv,
  stdenvNoCC,
  fetchurl,
  ...
}:

let
  system = stdenv.hostPlatform.system;

  archiveFile =
    {
      "aarch64-linux" = "nsh-linux-arm64.tar.gz";
      "x86_64-linux" = "nsh-linux-x86_64.tar.gz";
      "aarch64-darwin" = "nsh-macos-arm64.tar.gz";
    }
    .${system} or (throw "nsh does not support ${system}");
  archiveHash =
    {
      "aarch64-linux" = "sha256-2ZVQ7efkzjb3262f6dWn1OaX4dW8o4KOg+X9vbst5qw=";
      "x86_64-linux" = "sha256-pCK75dwqeuP/MI7lbgXXPVvsyB9XrfEUR/xXDc7mXg8=";
      "aarch64-darwin" = "sha256-5nIkJ7ORsAz37BMTfbk2bnjR56BF8pM88As0A2UwnZ0=";
    }
    .${system} or (throw "nsh does not support ${system}");
in

stdenvNoCC.mkDerivation (
  finalAttrs:
  let
    url = "https://github.com/longbridge/nsh/releases/download/v${finalAttrs.version}/${archiveFile}";
  in
  {
    pname = "nsh";
    version = "0.5.0";

    dontConfigure = true;
    dontBuild = true;
    dontStrip = true;

    src = fetchurl {
      inherit url;
      hash = archiveHash;
    };

    installPhase = ''
      runHook preInstall
      cp -r ../"$sourceRoot" "$out"
      runHook postInstall
    '';

    outputs = [ "out" ];

    passthru = { inherit url; };
  }
)
