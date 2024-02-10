---
tier: "1"
metadata:
    - pattern: "*"
      notes: "32-bit MinGW (Windows 7+)"
      std: true
      host: true
      footnotes:
        - name: "x86_32-floats-return-ABI"
          content: |
            Due to limitations of the C ABI, floating-point support on `i686` targets is non-compliant:
            floating-point return values are passed via an x87 register, so NaN payload bits can be lost.
            See [issue #114479][https://github.com/rust-lang/rust/issues/114479].
        - name: "windows-support"
          content: "Only Windows 10 currently undergoes automated testing. Earlier versions of Windows rely on testing and support from the community."
---

## Overview

32-bit Windows using MinGW.
