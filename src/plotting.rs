extern crate cpython;

use self::cpython::{PyInt, ToPyObject, Python, PyDict, PyResult, NoArgs};
use LineTiming;
use Duration;
use duration_to_nano;

impl ToPyObject for LineTiming {
    type ObjectType = PyDict;

    fn to_py_object(&self, py: Python) -> Self::ObjectType {
        let dict = PyDict::new(py);
        dict.set_item(py, "line_number", self.line_number).expect("failed to convert line number");
        let top_durs: Vec<(u32, f64)> = self.top_durations.iter().map(|(d, p)|{
            (duration_to_nano(&d), *p)
        }).collect();
        dict.set_item(py, "top_durations", top_durs).expect("failed to convert top durations");
        dict.set_item(py, "average_of_line", duration_to_nano(&self.average_of_line)).expect("failed to convert average of line");
        dict
    }
}

pub fn show(data: LineTiming){
    let gil = Python::acquire_gil();
    hello(gil.python(), data).unwrap();
}

fn hello(py: Python, data: LineTiming) -> PyResult<()> {
    let os = py.import("os")?;
    let cwd = os.call(py, "getcwd", NoArgs, None)?;
    let cwd = format!("{}{}", cwd, "/assets");
    
    let locals = PyDict::new(py);
    locals.set_item(py, "sys", py.import("sys")?)?;
    let add_assets_dir = format!("sys.path.append('{}')", cwd);
    py.eval(&add_assets_dir[..], None, Some(&locals))?;
    
    let chart = py.import("chart")?;
    chart.call(py, "show", (data,), None)?; 
    
    Ok(())
}
