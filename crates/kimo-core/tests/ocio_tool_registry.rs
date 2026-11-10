use kimo_core::commands::{dispatch,Session};
use serde_json::json;
#[test]fn ocio_tools_auto_registered(){let mut s=Session::new();let path=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/data/minimal_aces.ocio");let inspect=dispatch(&mut s,"inspect_ocio_config",&json!({"path":path}));
#[cfg(feature="native-ocio")]{let v=inspect.unwrap();assert_eq!(v["colorspaces"][0],"ACEScg");let result=dispatch(&mut s,"transform_ocio_samples",&json!({"path":path,"source":"ACEScg","destination":"ACEScct","rgba":[0.18,0.18,0.18,0.5]})).unwrap();assert!((result["rgba"][0].as_f64().unwrap()-0.4135884025).abs()<2e-6);assert_eq!(result["rgba"][3],0.5);assert!(dispatch(&mut s,"transform_ocio_samples",&json!({"path":path,"source":"ACEScg","destination":"ACEScct","rgba":[1e100,0.18,0.18,0.5]})).is_err());}
#[cfg(not(feature="native-ocio"))]{assert!(inspect.err().unwrap().contains("native-ocio"));}
}
