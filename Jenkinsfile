// This pipeline uses Groovy functions from https://github.com/juspay/jenkins-nix-ci
pipeline {
    agent none
    stages {
        stage ('Matrix') {
            matrix {
                agent {
                    label "${SYSTEM}"
                }
                when {
                    anyOf {
                        expression { 'x86_64-linux' == env.SYSTEM }
                        // Enable running macOS builds when on main branch, so
                        // as to provide Nix cache for people on macOS.
                        branch 'main'
                    }
                }
                axes {
                    axis {
                        name 'SYSTEM'
                        values 'x86_64-linux', 'aarch64-darwin', 'x86_64-darwin'
                    }
                }
                stages {
                    stage ('Cachix setup') {
                        steps {
                            cachixUse 'nammayatri'
                        }
                    }
                    stage ('Nix Build All') {
                        steps {
                            nixBuildAll system: env.SYSTEM
                        }
                    }
                    stage ('Cachix push') {
                        when {
                            anyOf {
                                branch 'main'
                            }
                        }
                        steps {
                            cachixPush "nammayatri"
                        }
                    }
                }
            }
        }
    }
}
