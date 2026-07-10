use regex::Regex;
use std::collections::HashMap;

/// Built-in dictionary of known technical terms and proper nouns with correct casing.
/// Applied as a post-pass after transcription/refinement.

pub fn multi_word_entries() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("mac os", "macOS"),
        ("open ai", "OpenAI"),
        ("chat gpt", "ChatGPT"),
        ("vs code", "VS Code"),
        ("node js", "Node.js"),
        ("next js", "Next.js"),
        ("visual studio", "Visual Studio"),
        ("pull request", "pull request"),
        ("builder hub", "BuilderHub"),
        ("git farm", "GitFarm"),
        ("cloud cover", "CloudCover"),
        ("monitor portal", "MonitorPortal"),
        ("cloud watch", "CloudWatch"),
        ("cloud front", "CloudFront"),
        ("cloud formation", "CloudFormation"),
        ("sage maker", "SageMaker"),
        ("phone tool", "Phonetool"),
        ("code browser", "Code Browser"),
        ("code search", "Code Search"),
        ("pr faq", "PR/FAQ"),
        ("q developer", "Q Developer"),
        ("amazon q", "Amazon Q"),
        ("s team", "S-team"),
        ("builder mcp", "Builder MCP"),
    ])
}

pub fn single_word_entries() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        // Cloud & AI
        ("github", "GitHub"), ("gitlab", "GitLab"), ("bitbucket", "Bitbucket"),
        ("aws", "AWS"), ("gcp", "GCP"), ("azure", "Azure"),
        ("openai", "OpenAI"), ("nvidia", "NVIDIA"), ("anthropic", "Anthropic"),
        ("claude", "Claude"), ("chatgpt", "ChatGPT"), ("gpt", "GPT"), ("llm", "LLM"),
        // Amazon internal
        ("brazil", "Brazil"), ("crux", "CRUX"), ("apollo", "Apollo"),
        ("builderhub", "BuilderHub"), ("viceroy", "Viceroy"), ("bemol", "Bemol"),
        ("cdk", "CDK"), ("gitfarm", "GitFarm"), ("hydra", "Hydra"), ("tod", "ToD"),
        ("cloudcover", "CloudCover"), ("carnaval", "Carnaval"),
        ("monitorportal", "MonitorPortal"), ("igraph", "iGraph"),
        ("dashbird", "Dashbird"), ("quip", "Quip"), ("phonetool", "Phonetool"),
        ("andes", "Andes"), ("midway", "Midway"), ("conduit", "Conduit"),
        ("isengard", "Isengard"), ("odin", "Odin"), ("bindles", "Bindles"),
        ("sim", "SIM"), ("taskei", "Taskei"), ("coral", "Coral"),
        ("smithy", "Smithy"), ("pippin", "Pippin"), ("kiro", "Kiro"),
        ("bedrock", "Bedrock"), ("sagemaker", "SageMaker"), ("asbx", "ASBX"),
        ("prfaq", "PR/FAQ"), ("sde", "SDE"), ("sdm", "SDM"), ("tpm", "TPM"),
        ("bie", "BIE"), ("mcm", "MCM"), ("coe", "COE"), ("sam", "SAM"),
        // AWS services
        ("ec2", "EC2"), ("s3", "S3"), ("sqs", "SQS"), ("sns", "SNS"),
        ("kinesis", "Kinesis"), ("ecs", "ECS"), ("eks", "EKS"), ("fargate", "Fargate"),
        ("cloudfront", "CloudFront"), ("cloudwatch", "CloudWatch"),
        ("cloudformation", "CloudFormation"),
        // Programming languages
        ("javascript", "JavaScript"), ("typescript", "TypeScript"),
        ("python", "Python"), ("ruby", "Ruby"), ("rust", "Rust"),
        ("golang", "Go"), ("kotlin", "Kotlin"), ("swift", "Swift"),
        // Linux/GNOME
        ("linux", "Linux"), ("gnome", "GNOME"), ("kde", "KDE"),
        ("wayland", "Wayland"), ("pipewire", "PipeWire"), ("pulseaudio", "PulseAudio"),
        ("systemd", "systemd"), ("flatpak", "Flatpak"), ("ubuntu", "Ubuntu"),
        ("fedora", "Fedora"), ("debian", "Debian"), ("archlinux", "Arch Linux"),
        // Apple platforms
        ("xcode", "Xcode"), ("macos", "macOS"), ("ios", "iOS"),
        ("ipados", "iPadOS"), ("iphone", "iPhone"), ("ipad", "iPad"),
        ("macbook", "MacBook"), ("swiftui", "SwiftUI"), ("whisperkit", "WhisperKit"),
        // Web & data
        ("api", "API"), ("apis", "APIs"), ("url", "URL"), ("urls", "URLs"),
        ("uri", "URI"), ("http", "HTTP"), ("https", "HTTPS"),
        ("json", "JSON"), ("xml", "XML"), ("yaml", "YAML"), ("csv", "CSV"),
        ("html", "HTML"), ("css", "CSS"), ("graphql", "GraphQL"),
        ("rest", "REST"), ("grpc", "gRPC"),
        // Databases
        ("sql", "SQL"), ("nosql", "NoSQL"), ("postgresql", "PostgreSQL"),
        ("postgres", "Postgres"), ("mysql", "MySQL"), ("mongodb", "MongoDB"),
        ("redis", "Redis"), ("dynamodb", "DynamoDB"),
        // Security & networking
        ("oauth", "OAuth"), ("jwt", "JWT"), ("ssh", "SSH"),
        ("tls", "TLS"), ("ssl", "SSL"), ("vpn", "VPN"), ("dns", "DNS"),
        ("ip", "IP"), ("tcp", "TCP"), ("udp", "UDP"),
        // Developer tools
        ("cli", "CLI"), ("gui", "GUI"), ("ui", "UI"), ("ux", "UX"),
        ("ide", "IDE"), ("sdk", "SDK"), ("npm", "npm"), ("pnpm", "pnpm"),
        ("yarn", "Yarn"), ("pip", "pip"), ("vscode", "VS Code"),
        ("jetbrains", "JetBrains"), ("intellij", "IntelliJ"),
        // Infrastructure
        ("docker", "Docker"), ("kubernetes", "Kubernetes"), ("k8s", "Kubernetes"),
        ("terraform", "Terraform"), ("ansible", "Ansible"), ("jenkins", "Jenkins"),
        // ML
        ("pytorch", "PyTorch"), ("tensorflow", "TensorFlow"),
        ("numpy", "NumPy"), ("pandas", "pandas"), ("huggingface", "Hugging Face"),
        // Frontend
        ("react", "React"), ("vue", "Vue"), ("angular", "Angular"),
        ("nextjs", "Next.js"), ("nodejs", "Node.js"),
    ])
}

/// Apply all known-term corrections to text (multi-word first, then single-word).
pub fn apply(text: &str) -> String {
    if text.is_empty() { return text.to_string(); }
    let mut result = text.to_string();

    // Multi-word replacements (longest first)
    let multi = multi_word_entries();
    let mut sorted: Vec<_> = multi.iter().collect();
    sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (key, value) in sorted {
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(key))) {
            result = re.replace_all(&result, *value).to_string();
        }
    }

    // Single-word replacements
    let singles = single_word_entries();
    let words: Vec<&str> = result.split(' ').collect();
    result = words.iter().map(|word| {
        let (core, trailing) = split_trailing_punct(word);
        if let Some(corrected) = singles.get(core.to_lowercase().as_str()) {
            format!("{}{}", corrected, trailing)
        } else {
            word.to_string()
        }
    }).collect::<Vec<_>>().join(" ");

    result
}

fn split_trailing_punct(word: &str) -> (&str, &str) {
    let end = word.trim_end_matches(|c: char| c.is_ascii_punctuation());
    let trailing = &word[end.len()..];
    (end, trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_word_correction() {
        assert_eq!(apply("I use aws and github"), "I use AWS and GitHub");
    }

    #[test]
    fn test_multi_word_correction() {
        assert_eq!(apply("I opened vs code"), "I opened VS Code");
    }

    #[test]
    fn test_preserves_punctuation() {
        assert_eq!(apply("check the api."), "check the API.");
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(apply("GITHUB is great"), "GitHub is great");
        assert_eq!(apply("using AWS already"), "using AWS already");
    }
}
