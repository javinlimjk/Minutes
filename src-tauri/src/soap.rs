use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ICD10Code {
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SOAPNote {
    pub specialty: String,
    pub subjective: String,
    pub objective: String,
    pub assessment: String,
    pub plan: String,
    pub icd10_suggestions: Vec<ICD10Code>,
    pub timestamp: String,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

pub fn get_specialty_prompt(specialty: &str) -> &'static str {
    match specialty.to_lowercase().as_str() {
        "psychiatry" => {
            "You are an expert psychiatric scribe. Focus on mental status exam (MSE), mood, affect, thought process, suicidal/homicidal ideation, past psychiatric history, and psychological interventions."
        }
        "pediatrics" => {
            "You are an expert pediatric scribe. Focus on developmental milestones, growth charts, parental observations, immunization status, pediatric physical exam, and age-appropriate treatment plans."
        }
        "orthopedics" => {
            "You are an expert orthopedic clinical scribe. Focus on musculoskeletal exam, range of motion (ROM), joint stability, gait, neurovascular status, imaging orders (X-ray/MRI), and physical therapy plans."
        }
        _ => {
            "You are an expert general practitioner clinical scribe. Focus on comprehensive history of present illness, physical exam, differential diagnosis, and evidence-based clinical management plans."
        }
    }
}

pub async fn generate_soap_note_local(
    endpoint: &str,
    model: &str,
    specialty: &str,
    transcript: &str,
    cloud_fallback_url: Option<String>,
    cloud_api_key: Option<String>,
) -> Result<SOAPNote, Box<dyn Error + Send + Sync>> {
    if transcript.trim().is_empty() {
        return Ok(empty_soap_note(specialty, "No dialogue recorded in session."));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let system_instructions = get_specialty_prompt(specialty);

    let prompt = format!(
        "{}\n\n\
        You are a clinical scribe assisting a healthcare provider. Your primary duty is STRICT FACTUAL FIDELITY.\n\
        Generate a SOAP Note based ONLY and EXCLUSIVELY on the spoken words present in the transcript below.\n\n\
        STRICT CLINICAL DOCUMENTATION RULES:\n\
        1. ABSOLUTE GROUNDEDNESS: Include ONLY facts, symptoms, conditions, medications, tests, and names explicitly stated in the transcript.\n\
        2. NO ASSUMPTIONS OR EXTRAPOLATIONS: Do NOT infer, guess, or assume patient age, demographics, occupations, family members, or past medical histories unless spoken directly in the consultation.\n\
        3. UNSTATED DEMOGRAPHICS: If the patient's age is not mentioned in the transcript, refer strictly to 'The patient' without stating an age.\n\
        4. OBJECTIVE: Record ONLY physical exam findings and vital signs explicitly measured during the consultation. If none were stated, write: 'Physical Examination: No specific vital signs or physical measurements stated during consultation.'\n\
        5. ASSESSMENT: Summarize the clinical impression and differential diagnosis based strictly on the symptoms described in the conversation.\n\
        6. PLAN: List only the laboratory tests, imaging, medication adjustments, and referrals discussed during the session.\n\
        7. LANGUAGE: Output the final clinical SOAP note in clear, professional English.\n\n\
        JSON FORMAT REQUIRED:\n\
        {{\n  \
          \"subjective\": \"Chief complaint, history of present illness, and reported symptoms strictly as spoken.\",\n  \
          \"objective\": \"Physical exam findings and vitals explicitly stated. If none stated, write 'Physical Examination: No specific vital signs or physical measurements stated during consultation.'\",\n  \
          \"assessment\": \"Clinical impression and primary differential diagnosis based strictly on discussed symptoms.\",\n  \
          \"plan\": \"Structured management plan based on the consultation.\",\n  \
          \"icd10_suggestions\": [\n    \
            {{\"code\": \"ICD-10 Code\", \"description\": \"Description of condition\"}}\n  \
          ]\n\
        }}\n\n\
        TRANSCRIPT:\n{}\n\n\
        Respond ONLY with a valid JSON object matching the schema above.",
        system_instructions, transcript
    );

    let request_body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "format": "json",
        "options": {
            "temperature": 0.0,
            "top_p": 0.1,
            "num_predict": 1024
        }
    });

    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));

    match client.post(&url).json(&request_body).send().await {
        Ok(resp) if resp.status().is_success() => {
            let res_json: OllamaResponse = resp.json().await?;
            if let Ok(note) = parse_soap_json(&res_json.response, specialty, transcript) {
                return Ok(note);
            }
        }
        _ => {}
    }

    // Try enterprise cloud API fallback if configured
    if let (Some(cloud_url), Some(api_key)) = (cloud_fallback_url, cloud_api_key) {
        if !cloud_url.trim().is_empty() && !api_key.trim().is_empty() {
            let cloud_payload = serde_json::json!({
                "model": "gpt-4o-mini",
                "messages": [
                    { "role": "system", "content": system_instructions },
                    { "role": "user", "content": prompt }
                ],
                "response_format": { "type": "json_object" }
            });

            if let Ok(resp) = client.post(&cloud_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&cloud_payload)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(val) = resp.json::<serde_json::Value>().await {
                        if let Some(content) = val["choices"][0]["message"]["content"].as_str() {
                            if let Ok(note) = parse_soap_json(content, specialty, transcript) {
                                return Ok(note);
                            }
                        }
                    }
                }
            }
        }
    }

    // If neither Local Ollama nor Cloud LLM succeeded, return a clear error
    Err(format!(
        "Failed to generate SOAP note: Local AI engine (Ollama at '{}') was unreachable or model '{}' is not loaded. Please ensure Ollama is running with '{}' or configure a Cloud API key in Settings.",
        endpoint, model, model
    ).into())
}

fn parse_soap_json(json_str: &str, specialty: &str, transcript: &str) -> Result<SOAPNote, Box<dyn Error + Send + Sync>> {
    let clean = json_str.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let val: serde_json::Value = serde_json::from_str(clean)?;

    let subjective = extract_json_field(&val, &["subjective", "Subjective", "S", "hpi"], "Subjective");
    let objective = extract_json_field(&val, &["objective", "Objective", "O", "exam"], "Objective");
    let assessment = extract_json_field(&val, &["assessment", "Assessment", "A", "impression"], "Assessment");
    let plan = extract_json_field(&val, &["plan", "Plan", "P", "treatment"], "Plan");

    let mut icd10_suggestions = Vec::new();
    let icd_val = &val["icd10_suggestions"];
    let arr_opt = icd_val.as_array().or_else(|| val["icd10"].as_array()).or_else(|| val["codes"].as_array());

    if let Some(arr) = arr_opt {
        for item in arr {
            let code = item["code"].as_str().or_else(|| item["icd10"].as_str()).unwrap_or("").to_string();
            let description = item["description"].as_str().or_else(|| item["desc"].as_str()).unwrap_or("").to_string();
            if !code.is_empty() {
                icd10_suggestions.push(ICD10Code { code, description });
            }
        }
    }

    if icd10_suggestions.is_empty() {
        icd10_suggestions = derive_icd10_codes(transcript);
    }

    Ok(SOAPNote {
        specialty: specialty.to_string(),
        subjective,
        objective,
        assessment,
        plan,
        icd10_suggestions,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

fn extract_json_field(val: &serde_json::Value, keys: &[&str], section_name: &str) -> String {
    for &key in keys {
        if let Some(v) = val.get(key) {
            if let Some(s) = v.as_str() {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            } else if let Some(arr) = v.as_array() {
                let lines: Vec<String> = arr.iter().filter_map(|item| item.as_str().map(|s| format!("• {}", s.trim()))).collect();
                if !lines.is_empty() {
                    return lines.join("\n");
                }
            }
        }
    }
    format!("No specific {} findings reported.", section_name)
}

fn derive_icd10_codes(transcript: &str) -> Vec<ICD10Code> {
    let lower = transcript.to_lowercase();
    let mut codes = Vec::new();

    if lower.contains("headache") || lower.contains("migraine") || lower.contains("head pain") {
        codes.push(ICD10Code { code: "R51.9".to_string(), description: "Headache, unspecified".to_string() });
    }
    if lower.contains("fever") || lower.contains("chills") || lower.contains("temperature") {
        codes.push(ICD10Code { code: "R50.9".to_string(), description: "Fever, unspecified".to_string() });
    }
    if lower.contains("cough") || lower.contains("sore throat") || lower.contains("phlegm") {
        codes.push(ICD10Code { code: "R05.9".to_string(), description: "Cough, unspecified".to_string() });
    }
    if lower.contains("hypertension") || lower.contains("blood pressure") || lower.contains("bp") {
        codes.push(ICD10Code { code: "I10".to_string(), description: "Essential (primary) hypertension".to_string() });
    }
    if lower.contains("back pain") || lower.contains("lumbar") || lower.contains("spine") {
        codes.push(ICD10Code { code: "M54.50".to_string(), description: "Low back pain, unspecified".to_string() });
    }
    if lower.contains("dementia") || lower.contains("memory loss") || lower.contains("memory") || lower.contains("cognitive") {
        codes.push(ICD10Code { code: "F03.90".to_string(), description: "Unspecified dementia, uncomplicated".to_string() });
    }

    if codes.is_empty() {
        codes.push(ICD10Code {
            code: "Z00.00".to_string(),
            description: "Encounter for general adult medical examination without abnormal findings".to_string(),
        });
    }

    codes
}

fn empty_soap_note(specialty: &str, msg: &str) -> SOAPNote {
    SOAPNote {
        specialty: specialty.to_string(),
        subjective: msg.to_string(),
        objective: "No objective physical data recorded.".to_string(),
        assessment: "Pending clinical consultation.".to_string(),
        plan: "Verify recording setup and microphone volume.".to_string(),
        icd10_suggestions: vec![ICD10Code {
            code: "Z00.00".to_string(),
            description: "General medical exam".to_string(),
        }],
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}
