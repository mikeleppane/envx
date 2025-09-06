use ahash::AHashMap as HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub name: String,
    pub description: String,
    pub category: String,
    pub variables: Vec<TemplateVariable>,
    pub profiles: HashMap<String, ProfileTemplate>,
    pub scripts: HashMap<String, ScriptTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    pub name: String,
    pub description: String,
    pub default: Option<String>,
    pub example: String,
    pub required: bool,
    pub sensitive: bool,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileTemplate {
    pub description: String,
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptTemplate {
    pub description: String,
    pub run: String,
    pub env: HashMap<String, String>,
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn get_builtin_templates() -> Vec<ProjectTemplate> {
    vec![
        // Next.js Full-Stack Template
        ProjectTemplate {
            name: "Next.js Full-Stack App".to_string(),
            description: "Next.js application with TypeScript and PostgreSQL".to_string(),
            category: "web".to_string(),
            variables: vec![
                TemplateVariable {
                    name: "DATABASE_URL".to_string(),
                    description: "PostgreSQL connection string".to_string(),
                    default: None,
                    example: "postgresql://user:pass@localhost:5432/myapp".to_string(),
                    required: true,
                    sensitive: true,
                    pattern: Some(r"^postgresql://.*".to_string()),
                },
                TemplateVariable {
                    name: "NEXTAUTH_URL".to_string(),
                    description: "NextAuth.js callback URL".to_string(),
                    default: Some("http://localhost:3000".to_string()),
                    example: "https://myapp.com".to_string(),
                    required: true,
                    sensitive: false,
                    pattern: Some(r"^https?://.*".to_string()),
                },
                TemplateVariable {
                    name: "NEXTAUTH_SECRET".to_string(),
                    description: "NextAuth.js secret for JWT encryption".to_string(),
                    default: None,
                    example: "your-secret-key".to_string(),
                    required: true,
                    sensitive: true,
                    pattern: None,
                },
            ],
            profiles: HashMap::from([
                (
                    "development".to_string(),
                    ProfileTemplate {
                        description: "Local development environment".to_string(),
                        variables: HashMap::from([
                            ("NODE_ENV".to_string(), "development".to_string()),
                            (
                                "NEXT_PUBLIC_API_URL".to_string(),
                                "http://localhost:3000/api".to_string(),
                            ),
                        ]),
                    },
                ),
                (
                    "production".to_string(),
                    ProfileTemplate {
                        description: "Production environment".to_string(),
                        variables: HashMap::from([
                            ("NODE_ENV".to_string(), "production".to_string()),
                            ("NEXT_PUBLIC_API_URL".to_string(), "https://api.myapp.com".to_string()),
                        ]),
                    },
                ),
            ]),
            scripts: HashMap::from([
                (
                    "dev".to_string(),
                    ScriptTemplate {
                        description: "Start development server".to_string(),
                        run: "npm run dev".to_string(),
                        env: HashMap::from([("NODE_ENV".to_string(), "development".to_string())]),
                    },
                ),
                (
                    "build".to_string(),
                    ScriptTemplate {
                        description: "Build for production".to_string(),
                        run: "npm run build".to_string(),
                        env: HashMap::from([("NODE_ENV".to_string(), "production".to_string())]),
                    },
                ),
            ]),
        },
        // Django + PostgreSQL Template
        ProjectTemplate {
            name: "Django + PostgreSQL".to_string(),
            description: "Django web application with PostgreSQL database".to_string(),
            category: "web".to_string(),
            variables: vec![
                TemplateVariable {
                    name: "SECRET_KEY".to_string(),
                    description: "Django secret key".to_string(),
                    default: None,
                    example: "django-insecure-...".to_string(),
                    required: true,
                    sensitive: true,
                    pattern: None,
                },
                TemplateVariable {
                    name: "DATABASE_URL".to_string(),
                    description: "PostgreSQL connection string".to_string(),
                    default: None,
                    example: "postgres://user:pass@localhost:5432/mydb".to_string(),
                    required: true,
                    sensitive: true,
                    pattern: Some(r"^postgres://.*".to_string()),
                },
                TemplateVariable {
                    name: "ALLOWED_HOSTS".to_string(),
                    description: "Comma-separated list of allowed hosts".to_string(),
                    default: Some("localhost,127.0.0.1".to_string()),
                    example: "myapp.com,www.myapp.com".to_string(),
                    required: true,
                    sensitive: false,
                    pattern: None,
                },
            ],
            profiles: HashMap::from([
                (
                    "development".to_string(),
                    ProfileTemplate {
                        description: "Local development".to_string(),
                        variables: HashMap::from([
                            ("DEBUG".to_string(), "True".to_string()),
                            ("DJANGO_SETTINGS_MODULE".to_string(), "myapp.settings.dev".to_string()),
                        ]),
                    },
                ),
                (
                    "production".to_string(),
                    ProfileTemplate {
                        description: "Production deployment".to_string(),
                        variables: HashMap::from([
                            ("DEBUG".to_string(), "False".to_string()),
                            ("DJANGO_SETTINGS_MODULE".to_string(), "myapp.settings.prod".to_string()),
                        ]),
                    },
                ),
            ]),
            scripts: HashMap::from([
                (
                    "migrate".to_string(),
                    ScriptTemplate {
                        description: "Run database migrations".to_string(),
                        run: "python manage.py migrate".to_string(),
                        env: HashMap::new(),
                    },
                ),
                (
                    "runserver".to_string(),
                    ScriptTemplate {
                        description: "Start development server".to_string(),
                        run: "python manage.py runserver".to_string(),
                        env: HashMap::new(),
                    },
                ),
            ]),
        },
        // Add more templates as needed...
    ]
}
