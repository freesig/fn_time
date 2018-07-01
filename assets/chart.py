import pygal

def show(data):
    line_chart = pygal.Line()
    line_chart.title = 'Timings'
    top_d = list(map(lambda x: x[1], data['top_durations']))
    for t in top_d:
        line_chart.add(str(data['line_number']), [t]) 
    line_chart.render_to_file('chart.svg')
