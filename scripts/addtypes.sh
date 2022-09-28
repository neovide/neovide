#!/usr/bin/env bash

set -Eeuf -o pipefail
shopt -s inherit_errexit
set -x

run() {
  local cmd=$1
  /usr/libexec/PlistBuddy -c "${cmd}" target/release/bundle/osx/Neovide.app/Contents/Info.plist
}

main() {
  plist=target/release/bundle/osx/Neovide.app/Contents/Info.plist

  if [[ -e "${plist}.bak" ]]; then
    # If the backup exists, overwrite the work copy during development to start fresh
    cp "${plist}"{.bak,}
  else
    # If the backup doesn't exist, create it
    cp "${plist}"{,.bak}
  fi

  run 'Add :CFBundleDocumentTypes array'

  run 'Add :CFBundleDocumentTypes:0:CFBundleTypeExtensions array'
  run 'Add :CFBundleDocumentTypes:0:CFBundleTypeExtensions:0 string txt'

  run 'Add :CFBundleDocumentTypes:0:CFBundleTypeMIMETypes array'
  run 'Add :CFBundleDocumentTypes:0:CFBundleTypeMIMETypes:0 string text/plain'

  run 'Add :CFBundleDocumentTypes:0:CFBundleTypeName string "Plain Text File"'
  run 'Add :CFBundleDocumentTypes:0:CFBundleTypeRole string Editor'

  run 'Add :CFBundleDocumentTypes:1:LSItemContentTypes array'
  run 'Add :CFBundleDocumentTypes:1:LSItemContentTypes:0 string public.text'
  run 'Add :CFBundleDocumentTypes:1:CFBundleTypeRole string Editor'
}
main "$@"
