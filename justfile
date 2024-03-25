# Installs files into the system
install: 
    sudo install -Dm0755 ./target/release/cosmic-applet-clipboard-manager  /usr/bin/cosmic-applet-clipboard-manager
    sudo install -Dm0644 data/dev.dominiccgeh.CosmicAppletClipboardManager.desktop /usr/share/applications/dev.dominiccgeh.CosmicAppletClipboardManager.desktop
    find 'data'/'icons' -type f -exec echo {} \; | rev | cut -d'/' -f-3 | rev | xargs -d '\n' -I {} sudo install -Dm0644 'data'/'icons'/{} /usr/share/icons/hicolor/{}